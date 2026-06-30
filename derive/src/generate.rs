use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ImplItem, ImplItemFn, ItemImpl, punctuated::Punctuated, visit_mut::VisitMut};

use crate::{
    DropImpl, DynImpl, ForeignItem, ForeignItemType,
    dispatch::{erase_handle_types, gen_dispatch_export},
    ffi_fn::{
        self, emit_extern_definition, gen_extern_fn_signature, merge_generics,
        normalize_fn_signature,
    },
    repr::gen_sized_family_impl,
    utils::{DispatchMonomorphizer, is_type_erased},
    wrapper::{
        gen_extern_decl, strip_internal_generic_attrs, wrap_fn_definition, wrap_impl_definition,
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OwnershipMode {
    #[default]
    Borrow,
    ByValue,
}

pub(crate) fn emit_decl_exports(abi: syn::Abi, decls: Vec<ForeignItem>) -> TokenStream {
    let exports = decls.into_iter().map(|decl| match decl {
        ForeignItem::Type(ForeignItemType {
            ty,
            id,
            dyn_self_impls,
            drop,
        }) => {
            let (impl_generics, ty_generics, where_clause) = ty.generics.split_for_impl();

            let params = &ty.generics.params;
            let predicates = ty.generics
                .where_clause
                .as_ref()
                .map(|w| &w.predicates);

            let ident = &ty.ident;
            let dispatch = dyn_self_impls
                .into_iter()
                .map(|dispatch| gen_dispatch_export(&abi, dispatch, id.as_deref()));

            let drop_impl = drop.as_ref().map(|drop| match drop {
                DropImpl::DynSelfImpl(item) => &item.impl_,
                DropImpl::DynImpl(item) => &item.impl_,
                DropImpl::Impl(impl_) => impl_,
            });

            let drop_check = gen_drop_impl_check(&ty, drop_impl.unwrap());
            let drop = drop.map(|drop| match drop {
                DropImpl::DynSelfImpl(item) => gen_dispatch_export(&abi, item, id.as_deref()),
                DropImpl::DynImpl(item) => gen_dispatch_export(&abi, item, None),
                DropImpl::Impl(impl_) => gen_drop_impl_definition(&abi, impl_),
            });

            let opaque = derive_opaque_item(id.as_deref(), ident, &ty.generics);
            let size_impl = gen_sized_family_impl(ident, &ty.generics);

            quote! {
                #opaque

                #drop
                #size_impl
                #drop_check

                impl #impl_generics co3::stored::EncodeOwned for #ident #ty_generics #where_clause {
                    type Store = ();

                    #[inline(always)]
                    fn soft_encode_owned<'itm>(self, (): &mut ()) -> Self::CType
                    where
                        Self: 'itm
                    {
                        self
                    }
                }
                impl<'_dšč, #params> co3::stored::DecodeOwned<'_dšč> for #ident #ty_generics where
                    Self: '_dšč,
                    #predicates
                {
                    type Store = ();

                    #[inline(always)]
                    unsafe fn soft_decode_owned<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Option<Self> {
                        Some(source)
                    }
                }

                unsafe impl #impl_generics co3::borrow::BorrowCast for #ident #ty_generics #where_clause {
                    type AsConst = Self;
                }
                unsafe impl #impl_generics co3::borrow::BorrowCastMut for #ident #ty_generics #where_clause {
                    type AsMut = Self;
                }

                #(#dispatch)*
            }
        }
        ForeignItem::Fn(item) => ffi_fn::gen_fn_definition(&abi, item),
        ForeignItem::Impl(impl_) => ffi_fn::gen_impl_definition(&abi, impl_),
        ForeignItem::DynImpl(item) => gen_dispatch_export(&abi, item, None),
    });

    quote! { #( const _: () = { #exports }; )* }
}

pub(crate) fn expand_extern_import_decls(
    abi: syn::Abi,
    attrs: &[syn::Attribute],
    decls: Vec<ForeignItem>,
) -> TokenStream {
    fn expand_extern_dispatch_impl(
        impl_: &ItemImpl,
        args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
    ) -> Vec<TokenStream> {
        if args.is_empty() {
            return vec![quote!(#impl_)];
        }

        args.iter()
            .map(|entry| {
                let mut monomorphized = impl_.clone();
                monomorphized.generics.params.clear();

                DispatchMonomorphizer::new(&impl_.generics, entry)
                    .visit_item_impl_mut(&mut monomorphized);

                quote! { #monomorphized }
            })
            .collect()
    }

    fn gen_impl_extern_fn_decls(
        abi: &syn::Abi,
        attrs: &[syn::Attribute],
        impl_: ItemImpl,
        self_id: Option<&syn::Type>,
        args: Option<&Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>>,
    ) -> Vec<TokenStream> {
        impl_
            .items
            .into_iter()
            .filter_map(|item| {
                let syn::ImplItem::Fn(mut item) = item else {
                    return None;
                };

                normalize_fn_signature(&mut item.sig, Some(&impl_.self_ty));
                merge_generics(impl_.generics.clone(), &mut item.sig.generics);

                if let Some(args) = args {
                    erase_handle_types(&impl_.generics, self_id, &mut item.sig, args);
                }

                let decl = gen_extern_fn_signature(item.sig);
                Some(gen_extern_decl(abi, attrs, &item.attrs, decl))
            })
            .collect()
    }

    fn expand_impl_import(
        abi: &syn::Abi,
        attrs: &[syn::Attribute],
        impl_: ItemImpl,
    ) -> TokenStream {
        let import = wrap_impl_definition::<false>(&impl_);
        let extern_decl = gen_impl_extern_fn_decls(abi, attrs, impl_, None, None);

        quote! {
            const _: () = {
                #(#extern_decl)*
                #import
            };
        }
    }

    fn expand_dispatch_import(
        abi: &syn::Abi,
        attrs: &[syn::Attribute],
        self_id: Option<&syn::Type>,
        dispatch: DynImpl,
    ) -> TokenStream {
        let DynImpl { impl_, args, .. } = dispatch;
        let dispatch_helper = gen_dispatch_helper(&impl_.generics, &args);

        let wrapped = wrap_impl_definition::<true>(&impl_);
        let imports = expand_extern_dispatch_impl(&wrapped, &args);
        let extern_decl = gen_impl_extern_fn_decls(abi, attrs, impl_, self_id, Some(&args));

        quote! {
            const _: () = {
                #dispatch_helper

                #(#extern_decl)*
                #(#imports)*
            };
        }
    }

    let imports = decls.into_iter().map(|decl| match decl {
        ForeignItem::Type(ForeignItemType {
            ty,
            id,
            dyn_self_impls,
            drop,
        }) => {
            let ty = wrap_extern_type_decl(&abi, attrs, id.as_deref(), ty);

            let dispatch = dyn_self_impls
                .into_iter()
                .map(|dispatch| expand_dispatch_import(&abi, attrs, id.as_deref(), dispatch));

            let drop = drop.map(|drop| match drop {
                DropImpl::DynSelfImpl(d) | DropImpl::DynImpl(d) => {
                    expand_dispatch_drop_import(&abi, attrs, d.impl_, id.as_deref().unwrap())
                }
                DropImpl::Impl(impl_) => expand_impl_import(&abi, attrs, impl_),
            });

            quote! {
                #ty
                #drop
                #(#dispatch)*
            }
        }
        ForeignItem::Fn(item) => wrap_fn_definition(&abi, attrs, item),
        ForeignItem::Impl(impl_) => expand_impl_import(&abi, attrs, impl_),
        ForeignItem::DynImpl(dispatch) => expand_dispatch_import(&abi, attrs, None, dispatch),
    });

    quote! { #(#imports)* }
}

pub(crate) fn gen_handle_family_impl(
    ident: &syn::Ident,
    generics: &syn::Generics,
    id: &syn::Type,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics co3::handle::HandleFamily for #ident #ty_generics #where_clause {
            type Kind = #id;
        }
    }
}

fn gen_dispatch_helper(
    generics: &syn::Generics,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) -> Option<TokenStream> {
    let erased_params = generics
        .type_params()
        .filter(|p| p.attrs.iter().any(is_type_erased))
        .collect::<Vec<_>>();

    let erased_tys = erased_params.iter().map(|p| &p.ident);
    let erased_generics = erased_params.iter().map(|param| {
        let mut param = (*param).clone();
        param.attrs.retain(|attr| !is_type_erased(attr));
        quote!(#param)
    });

    let dispatch_checks = args.iter().map(|entry| {
        let erased_args = generics
            .params
            .iter()
            .filter(|param| !matches!(param, syn::GenericParam::Lifetime(_)))
            .zip(&entry.args)
            .filter_map(|(param, arg)| match param {
                syn::GenericParam::Type(param) if param.attrs.iter().any(is_type_erased) => {
                    Some(quote!(#arg))
                }
                _ => None,
            });

        quote! {
            const _: __Co3DispatchParams::<#(#erased_args),*> =
                __Co3DispatchParams(core::marker::PhantomData);
        }
    });

    Some(quote! {
        // NOTE: Verifies `?Sized` bounds on erased params
        struct __Co3DispatchParams<#(#erased_generics),*>(
            core::marker::PhantomData<(#(*const #erased_tys),*)>
        );

        #(#dispatch_checks)*
    })
}

fn gen_drop_impl_definition(abi: &syn::Abi, mut impl_: ItemImpl) -> TokenStream {
    let self_ty = &impl_.self_ty;

    let Some(syn::ImplItem::Fn(mut item)) = impl_.items.pop() else {
        unreachable!()
    };

    let ffi_fn_body = quote! {{
        let __co3_self: &mut #self_ty = unsafe {
            co3::Decode::decode(__co3_self)
        }.ok_or(co3::FfiReturn::TrapRepresentation)?;

        unsafe { core::ptr::drop_in_place(__co3_self as *mut _) };

        Ok(())
    }};

    normalize_fn_signature(&mut item.sig, Some(self_ty));
    merge_generics(impl_.generics.clone(), &mut item.sig.generics);
    let fn_signature = gen_extern_fn_signature(item.sig);
    emit_extern_definition(abi, &item.attrs, fn_signature, ffi_fn_body)
}

fn expand_dispatch_drop_import(
    abi: &syn::Abi,
    attrs: &[syn::Attribute],
    impl_: ItemImpl,
    id_ty: &syn::Type,
) -> TokenStream {
    let ItemImpl {
        attrs: impl_attrs,
        generics,
        self_ty,
        items,
        ..
    } = &impl_;

    let (impl_generics, _, _) = generics.split_for_impl();
    let predicates = generics.where_clause.as_ref().map(|w| &w.predicates);

    let ImplItem::Fn(ImplItemFn {
        attrs: wrapper_attrs,
        sig,
        ..
    }) = items.iter().next().unwrap()
    else {
        unreachable!()
    };

    let link_name = wrapper_attrs
        .iter()
        .find(|attr| attr.path().is_ident("link_name"));
    let wrapper_attrs = wrapper_attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("link_name"));

    let handle_id_conversion_stmts = sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(syn::PatType { pat, .. }) = arg {
                Some(quote! {
                    let #pat: #id_ty = {
                        // FIXME: https://github.com/mversic/co3/issues/93
                        // Should it be required that HandleFamily::Kind: Copy
                        let __co3_handle_id = <Self as co3::handle::Handle>::ID;
                        unsafe { core::mem::transmute_copy(&__co3_handle_id)
                    }};
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let (inputs, values): (Vec<_>, Vec<_>) = sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Receiver(_) => (quote!(*mut core::ffi::c_void), quote!(__co3_self)),
            FnArg::Typed(syn::PatType { pat, .. }) => {
                (quote!(<#id_ty as co3::ExternC>::CType), quote!(#pat))
            }
        })
        .unzip();

    let bounds = inputs.iter().map(|ty| {
        quote! { #ty: co3::CFnArg }
    });
    quote! {
        #(#impl_attrs)*
        impl #impl_generics Drop for #self_ty where
            Self: co3::handle::Handle,
            #predicates
        {
            #(#wrapper_attrs)*
            fn drop(&mut self) {
                unsafe #abi {
                    #(#attrs)*

                    #link_name
                    fn drop(#(#values: #inputs),*) -> co3::FfiReturn where #(#bounds,)*;
                }

                let __co3_self = self as *mut #self_ty as *mut core::ffi::c_void;
                #(#handle_id_conversion_stmts)*
                unsafe { drop(#(#values),*) };
            }
        }
    }
}

pub(crate) fn is_unsafe_no_mangle(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("unsafe") {
        return false;
    }

    let syn::Meta::List(meta_list) = &attr.meta else {
        return false;
    };

    let Ok(metas) =
        meta_list.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
    else {
        return false;
    };

    metas.into_iter().any(|meta| match meta {
        syn::Meta::Path(path) => path.is_ident("no_mangle"),
        _ => false,
    })
}

fn gen_drop_impl_check(item: &syn::ForeignItemType, impl_: &ItemImpl) -> TokenStream {
    let mut impl_generics = impl_.generics.clone();
    strip_internal_generic_attrs(&mut impl_generics);

    let ident = &item.ident;
    let self_ty = &impl_.self_ty;

    let item_attrs = impl_
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("dispatch"));

    let marker_fields = item.generics.params.iter().map(|param| match param {
        syn::GenericParam::Lifetime(param) => {
            let lifetime = &param.lifetime;
            quote!(core::marker::PhantomData<&#lifetime ()>)
        }
        syn::GenericParam::Type(param) => {
            let ident = &param.ident;
            quote!(core::marker::PhantomData<#ident>)
        }
        syn::GenericParam::Const(param) => {
            let ident = &param.ident;
            quote!([(); #ident])
        }
    });

    let (decl_generics, _, item_where_clause) = item.generics.split_for_impl();
    let (impl_generics, _, where_clause) = impl_generics.split_for_impl();

    quote! {{
        #(#item_attrs)*
        struct #ident #decl_generics (#(#marker_fields),*) #item_where_clause;

        impl #impl_generics Drop for #self_ty #where_clause {
            fn drop(&mut self) {}
        }
    }}
}

fn wrap_extern_type_decl(
    _abi: &syn::Abi,
    _attrs: &[syn::Attribute],
    id: Option<&syn::Type>,
    mut type_: syn::ForeignItemType,
) -> TokenStream {
    let has_non_lifetime_generics = type_
        .generics
        .params
        .iter()
        .any(|param| !matches!(param, syn::GenericParam::Lifetime(_)));

    if has_non_lifetime_generics {
        let ident = &type_.ident;
        let generics = &type_.generics;
        let (_, ty_generics, _) = generics.split_for_impl();
        let extern_type = quote! { #ident #ty_generics };
        type_
            .generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote! { #extern_type: co3::handle::Handle });
    }

    let syn::ForeignItemType {
        attrs: type_attrs,
        generics,
        vis,
        ident,
        ..
    } = type_;

    let owned_ident = gen_owned_extern_type_name(&ident);
    let owned_doc = gen_owned_extern_type_doc(&ident);
    let owned_repr_c_name = gen_owned_repr_c_name(&ident);
    let owned_repr_c_doc = format!("FFI-safe representation of `{owned_ident}`");
    let ident_impls = gen_extern_type_impls(id, &ident, &generics);
    let owned_impls = gen_owned_extern_type_impls(&ident, &generics);
    let owned_repr_c_impls = gen_owned_repr_c_impls(&ident, &generics);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    use syn::GenericParam::*;
    let phantom_data_fields = generics.params.iter().filter_map(|param| match param {
        Lifetime(param) => {
            let lifetime = &param.lifetime;
            Some(quote! { core::marker::PhantomData<&#lifetime mut ()> })
        }
        Type(param) => {
            let ident = &param.ident;
            Some(quote! { core::marker::PhantomData<#ident> })
        }
        Const(_) => None,
    });

    quote! {
        #(#type_attrs)*
        #[repr(C)]
        #vis struct #ident #impl_generics #where_clause {
            // FIXME: Is this the correct way to declare extern type
            // https://doc.rust-lang.org/nomicon/ffi.html#representing-opaque-structs
            // FIXME: How do data fields affect alignment here?
            data: core::marker::PhantomData<(#(#phantom_data_fields),*)>,
            __marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
        }

        // FIXME: https://github.com/rust-lang/rust/issues/43467
        //unsafe #abi {
        //    #(#attrs)*

        //    #(#type_attrs)*
        //    #vis type #ident #impl_generics #where_clause;
        //}

        #[doc = #owned_doc]
        #[repr(transparent)]
        #vis struct #owned_ident #impl_generics (*mut #ident #ty_generics) #where_clause;

        #[doc(hidden)]
        #[repr(transparent)]
        #[doc = #owned_repr_c_doc]
        #vis struct #owned_repr_c_name #impl_generics (*mut #ident #ty_generics) #where_clause;

        impl #impl_generics Drop for #owned_ident #ty_generics #where_clause {
            fn drop(&mut self) {
                unsafe { core::ptr::drop_in_place(self.0) }
            }
        }

        #ident_impls
        #owned_impls
        #owned_repr_c_impls
    }
}

fn gen_owned_extern_type_name(ident: &syn::Ident) -> syn::Ident {
    format_ident!("Owned{ident}")
}

fn gen_owned_repr_c_name(ident: &syn::Ident) -> syn::Ident {
    format_ident!("C{}", gen_owned_extern_type_name(ident))
}

fn gen_owned_extern_type_doc(ident: &syn::Ident) -> String {
    format!("Owned representation of `{ident}`")
}

fn gen_extern_type_impls(
    id: Option<&syn::Type>,
    ident: &syn::Ident,
    generics: &syn::Generics,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let opaque_impls = derive_opaque_item(id, ident, generics);

    quote! {
        #opaque_impls

        impl #impl_generics co3::size::SizeFamily for #ident #ty_generics #where_clause {
            type Kind = co3::size::ExternTypeLike;
        }

        unsafe impl #impl_generics co3::borrow::BorrowCast for #ident #ty_generics #where_clause {
            type AsConst = Self;
        }
        unsafe impl #impl_generics co3::borrow::BorrowCastMut for #ident #ty_generics #where_clause {
            type AsMut = Self;
        }
    }
}

fn gen_owned_repr_c_impls(ident: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;
    let owned_repr_c_name = gen_owned_repr_c_name(ident);
    let size_impl = gen_sized_family_impl(&owned_repr_c_name, generics);

    quote! {
        impl #impl_generics #owned_repr_c_name #ty_generics #where_clause {
            fn is_none(&self) -> bool {
                self.0.is_null()
            }
        }

        #size_impl
        impl #impl_generics co3::ir::ReprFamily for #owned_repr_c_name #ty_generics #where_clause {
            type Kind = co3::ir::ReprC<co3::ir::Robust>;
        }
        impl #impl_generics co3::niche::NicheFamily for #owned_repr_c_name #ty_generics #where_clause {
            type Kind = co3::niche::WithoutNiche;
        }

        unsafe impl #impl_generics co3::RobustReprC for #owned_repr_c_name #ty_generics #where_clause {}
        unsafe impl #impl_generics co3::CFnArg for #owned_repr_c_name #ty_generics #where_clause {}

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #owned_repr_c_name #ty_generics #where_clause {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        impl #impl_generics co3::ExternC for #owned_repr_c_name #ty_generics #where_clause {
            type CType = Self;
        }
        impl #impl_generics co3::stored::EncodeOwned for #owned_repr_c_name #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            fn soft_encode_owned<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'d, #params> co3::stored::DecodeOwned<'d> for #owned_repr_c_name #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode_owned<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl #impl_generics co3::Encode for #owned_repr_c_name #ty_generics #where_clause {}
        impl #impl_generics co3::Decode<'_> for #owned_repr_c_name #ty_generics #where_clause {}

        unsafe impl #impl_generics co3::borrow::BorrowCast for #owned_repr_c_name #ty_generics #where_clause {
            type AsConst = *const #ident #ty_generics;
        }
        unsafe impl #impl_generics co3::borrow::BorrowCastMut for #owned_repr_c_name #ty_generics #where_clause {
            type AsMut = *mut #ident #ty_generics;
        }

        impl #impl_generics Clone for #owned_repr_c_name #ty_generics #where_clause {
            fn clone(&self) -> Self { *self }
        }
        impl #impl_generics Copy for #owned_repr_c_name #ty_generics #where_clause {}
    }
}

fn gen_owned_extern_type_impls(ident: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;

    let owned_ident = gen_owned_extern_type_name(ident);
    let owned_repr_c_name = gen_owned_repr_c_name(ident);

    let size_impl = gen_sized_family_impl(&owned_ident, generics);

    quote! {
        #size_impl

        impl #impl_generics co3::ir::ReprFamily for #owned_ident #ty_generics #where_clause {
            type Kind = co3::ir::ReprC<co3::ir::NonRobust>;
        }
        impl #impl_generics co3::niche::NicheFamily for #owned_ident #ty_generics #where_clause {
            type Kind = co3::niche::WithStableNiche;
        }

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #owned_ident #ty_generics #where_clause {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                // NOTE: Null pointer is validated although it's not strictly required
                // Opaque pointers should never be dereferenced, this catches mistakes
                // TODO: Just return true?
                !target.is_none()
            }
        }

        impl #impl_generics co3::ExternC for #owned_ident #ty_generics #where_clause {
            type CType = #owned_repr_c_name #ty_generics;
        }
        impl #impl_generics co3::niche::Niche for #owned_ident #ty_generics #where_clause {
            const NICHE_VALUE: Self::CType = #owned_repr_c_name(core::ptr::null_mut());
        }

        unsafe impl #impl_generics co3::niche::StableNiche for #owned_ident #ty_generics #where_clause {}

        impl #impl_generics co3::stored::EncodeOwned for #owned_ident #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            fn soft_encode_owned<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                #owned_repr_c_name(core::mem::ManuallyDrop::new(self).0)
            }
        }
        impl<'d, #params> co3::stored::DecodeOwned<'d> for #owned_ident #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode_owned<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                unsafe { <Self as co3::transmute::CheckedTransmute>::is_valid(&source) }.then_some(Self(source.0))
            }
        }

        impl #impl_generics co3::Encode for #owned_ident #ty_generics #where_clause {}
        impl #impl_generics co3::Decode<'_> for #owned_ident #ty_generics #where_clause {}

        unsafe impl #impl_generics co3::handle::Erase for #owned_ident #ty_generics #where_clause {
            type Erased = *mut core::ffi::c_void;
        }

        impl #impl_generics core::ops::Deref for #owned_ident #ty_generics #where_clause {
            type Target = #ident #ty_generics;

            fn deref(&self) -> &Self::Target {
                unsafe { &*self.0 }
            }
        }
        impl #impl_generics core::ops::DerefMut for #owned_ident #ty_generics #where_clause {
            fn deref_mut(&mut self) -> &mut Self::Target {
                unsafe { &mut *self.0 }
            }
        }

        impl #impl_generics core::convert::AsRef<#ident #ty_generics> for #owned_ident #ty_generics #where_clause {
            fn as_ref(&self) -> &#ident #ty_generics {
                self
            }
        }
        impl #impl_generics core::convert::AsMut<#ident #ty_generics> for #owned_ident #ty_generics #where_clause {
            fn as_mut(&mut self) -> &mut #ident #ty_generics {
                self
            }
        }

        impl #impl_generics core::borrow::Borrow<#ident #ty_generics> for #owned_ident #ty_generics #where_clause {
            fn borrow(&self) -> &#ident #ty_generics {
                self
            }
        }
        impl #impl_generics core::borrow::BorrowMut<#ident #ty_generics> for #owned_ident #ty_generics #where_clause {
            fn borrow_mut(&mut self) -> &mut #ident #ty_generics {
                self
            }
        }
    }
}

fn derive_opaque_item(
    id: Option<&syn::Type>,
    ident: &syn::Ident,
    generics: &syn::Generics,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let handle_family_impl = id.map(|id| gen_handle_family_impl(ident, generics, id));

    quote! {
        #handle_family_impl

        impl #impl_generics co3::ir::ReprFamily for #ident #ty_generics #where_clause {
            type Kind = co3::ir::ReprC<co3::ir::Robust>;
        }

        unsafe impl #impl_generics co3::RobustReprC for #ident #ty_generics #where_clause {}

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #ident #ty_generics #where_clause {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        impl #impl_generics co3::ExternC for #ident #ty_generics #where_clause {
            type CType = Self;
        }

        unsafe impl #impl_generics co3::handle::Erase for #ident #ty_generics #where_clause {
            type Erased = core::ffi::c_void;
        }
    }
}
