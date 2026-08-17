use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    FnArg, ImplItem, ImplItemFn, ItemImpl, punctuated::Punctuated, visit::Visit,
    visit_mut::VisitMut,
};

use crate::{
    Co3Fn, Co3Impl, DispatchGroups, ForeignItem, ForeignItemType, co3_path,
    dispatch::{
        StaticLifetimeNormalizer, erase_handle_types, gen_dispatch_erased_layout_checks,
        gen_dispatch_export, gen_dispatch_fn_export, gen_handle_id_uniqueness_checks,
    },
    ffi_fn::{
        self, emit_extern_definition, gen_extern_fn_signature, merge_generics,
        normalize_fn_signature, strip_dispatch_params,
    },
    parse::{FailureMode, MacroFeatures},
    symbol_name_value,
    utils::{
        DispatchMonomorphizer, ParamUseDetector, cfg_attrs, erased_id_repr,
        has_non_lifetime_generics, is_type_erased, soft_for_arg, strip_internal_generic_param,
    },
    wrapper::{
        gen_extern_decl, gen_wrapper_body, strip_internal_arg_attrs, wrap_fn_definition,
        wrap_impl_definition,
    },
};

fn is_dyn_self_dispatch_for(impl_: &ItemImpl, declared_type: &syn::Ident) -> bool {
    let Some(syn::TraitBound { path, .. }) = crate::trait_object_single_trait_bound(&impl_.self_ty)
    else {
        return false;
    };

    path.segments.len() == 1
        && path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == *declared_type)
}

fn lift_dispatch_method(mut dispatch: Co3Impl) -> Co3Impl {
    let syn::ImplItem::Fn(method) = dispatch.items.first_mut().unwrap() else {
        unreachable!()
    };
    let method_generics = core::mem::take(&mut method.sig.generics);
    dispatch.generics.params.extend(method_generics.params);
    if let Some(where_clause) = method_generics.where_clause {
        dispatch
            .generics
            .make_where_clause()
            .predicates
            .extend(where_clause.predicates);
    }

    dispatch
}

fn split_dyn_methods(mut impl_: Co3Impl) -> (Co3Impl, Vec<Co3Impl>) {
    let impl_dispatch_args = impl_.dispatch_args.clone();
    let mut plain_items = Vec::new();
    let mut dispatched = Vec::new();
    for item in core::mem::take(&mut impl_.item.items) {
        let syn::ImplItem::Fn(method) = item else {
            plain_items.push(item);
            continue;
        };
        let Some(method_dispatch_args) = impl_.method_dispatch_args.remove(&method.sig.ident)
        else {
            plain_items.push(syn::ImplItem::Fn(method));
            continue;
        };
        let mut method_impl = impl_.item.clone();
        method_impl.items = vec![syn::ImplItem::Fn(method)];
        dispatched.push(Co3Impl {
            item: method_impl,
            dispatch_args: impl_dispatch_args.combined_with(&method_dispatch_args),
            method_dispatch_args: Default::default(),
        });
    }
    impl_.item.items = plain_items;
    (impl_, dispatched)
}

fn dispatches_self(item: &Co3Impl) -> bool {
    item.items.iter().any(|item| {
        let syn::ImplItem::Fn(method) = item else {
            return false;
        };

        method.sig.inputs.iter().any(|input| {
            let syn::FnArg::Typed(arg) = input else {
                return false;
            };

            matches!(
                crate::dispatch::handle_id(&arg.ty),
                Some(crate::dispatch::HandleId::DynSelf)
            )
        })
    })
}

fn gen_export_impl(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    impl_: Co3Impl,
    type_id: Option<&syn::Type>,
) -> TokenStream {
    let (plain, methods) = split_dyn_methods(impl_);
    let plain = if plain.items.is_empty() {
        TokenStream::new()
    } else if !plain.dispatch_args.is_empty() {
        let self_id = dispatches_self(&plain).then_some(type_id).flatten();
        gen_dispatch_export(abi, failure_mode, plain, self_id)
    } else {
        ffi_fn::gen_impl_definition(abi, failure_mode, plain.item)
    };
    let methods = methods.into_iter().map(|method| {
        let self_id = dispatches_self(&method).then_some(type_id).flatten();
        gen_dispatch_export(abi, failure_mode, lift_dispatch_method(method), self_id)
    });

    quote!(#plain #(#methods)*)
}

fn dispatch_type_idents(generics: &syn::Generics) -> Vec<syn::Ident> {
    generics
        .type_params()
        .filter(|param| param.attrs.iter().any(is_type_erased))
        .map(|param| param.ident.clone())
        .collect()
}

fn dispatch_set_name(attrs: &[syn::Attribute], fallback: &syn::Ident) -> syn::Ident {
    let _ = attrs;
    format_ident!("__Co3DispatchSet_{fallback}")
}

fn dispatch_module_name(attrs: &[syn::Attribute], fallback: &syn::Ident) -> syn::Ident {
    let symbol = attrs.iter().find_map(symbol_name_value).and_then(|value| {
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) = value
        else {
            return None;
        };
        Some(value.value())
    });
    let name = symbol.unwrap_or_else(|| fallback.to_string());
    let name = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    format_ident!("__co3_dispatch_{name}")
}

fn gen_dispatch_set(
    trait_name: &syn::Ident,
    generics: &syn::Generics,
    args: &DispatchGroups,
    member_tys: impl IntoIterator<Item = syn::Type>,
) -> TokenStream {
    struct LifetimeCollector(BTreeSet<syn::Ident>);

    impl<'ast> Visit<'ast> for LifetimeCollector {
        fn visit_lifetime(&mut self, lifetime: &'ast syn::Lifetime) {
            if lifetime.ident != "static" && lifetime.ident != "_" {
                self.0.insert(lifetime.ident.clone());
            }
        }
    }

    let member_tys = member_tys.into_iter().collect::<Vec<_>>();
    let mut impls = Vec::new();
    args.for_each_combination(|selections| {
        let mut tys = member_tys.clone();
        let mut monomorphizer = DispatchMonomorphizer::for_dispatch_group(generics, selections);
        tys.iter_mut()
            .for_each(|ty| monomorphizer.visit_type_mut(ty));
        let mut used = LifetimeCollector(BTreeSet::new());
        for ty in &tys {
            used.visit_type(ty);
        }

        let lifetimes = generics
            .lifetimes()
            .filter(|param| used.0.contains(&param.lifetime.ident));
        let lifetimes = lifetimes.collect::<Vec<_>>();
        let impl_generics = (!lifetimes.is_empty()).then(|| quote!(<#(#lifetimes),*>));

        impls.push(quote! { impl #impl_generics #trait_name for (#(#tys,)*) {} });
    });

    quote! {
        trait #trait_name {}
        #(#impls)*
    }
}

fn prepare_dispatch_wrapper_sig(
    sig: &syn::Signature,
    dispatch_generics: &syn::Generics,
    dispatch_set: &syn::Ident,
    co3: &TokenStream,
    membership_tys: impl IntoIterator<Item = syn::Type>,
) -> syn::Signature {
    let mut wrapper_sig = sig.clone();
    wrapper_sig.inputs = wrapper_sig
        .inputs
        .into_iter()
        .filter(|input| !crate::dispatch::is_handle_id_arg(input))
        .collect();
    strip_internal_arg_attrs(&mut wrapper_sig);
    wrapper_sig
        .generics
        .type_params_mut()
        .for_each(strip_internal_generic_param);

    let dispatch_tys = dispatch_type_idents(dispatch_generics);
    let membership_tys = membership_tys.into_iter().collect::<Vec<_>>();
    let where_clause = wrapper_sig.generics.make_where_clause();
    where_clause
        .predicates
        .push(syn::parse_quote!((#(#membership_tys,)*): #dispatch_set));

    for ident in &dispatch_tys {
        let id_repr = dispatch_generics
            .type_params()
            .find(|param| param.ident == *ident)
            .and_then(erased_id_repr)
            .expect("dispatch type has an ID representation");
        where_clause.predicates.push(syn::parse_quote!(
            #ident: #co3::handle::Handle
        ));
        where_clause.predicates.push(syn::parse_quote!(
            <#ident as #co3::handle::HandleFamily>::Kind:
                #co3::Encode<CType = #id_repr, Store: #co3::stored::EmptyStore>
        ));
    }

    let detector = ParamUseDetector::new(dispatch_tys.iter());
    for input in &sig.inputs {
        let FnArg::Typed(input) = input else { continue };
        let (attrs, ty) = (&input.attrs[..], &*input.ty);
        if crate::dispatch::handle_id(ty).is_none() && detector.type_mentions_param(ty) {
            if attrs.iter().any(ffi_fn::is_by_val_attr) {
                let bound = if soft_for_arg(attrs) {
                    syn::parse_quote!(#ty: #co3::Encode)
                } else {
                    syn::parse_quote!(#ty: #co3::Encode<Store: #co3::stored::EmptyStore>)
                };
                where_clause.predicates.push(bound);
            } else {
                where_clause
                    .predicates
                    .push(syn::parse_quote!(#ty: #co3::ExternC + #co3::borrow::Borrow));
                where_clause.predicates.push(syn::parse_quote!(
                    <#ty as #co3::ExternC>::CType: #co3::borrow::BorrowCast<AsConst: Sized>
                ));
                let bound = if soft_for_arg(attrs) {
                    syn::parse_quote!(
                        for<'__co3_borrow> <#ty as #co3::borrow::Borrow>::Borrowed<'__co3_borrow>:
                            #co3::Encode<
                                CType = <<#ty as #co3::ExternC>::CType as #co3::borrow::BorrowCast>::AsConst,
                            >
                    )
                } else {
                    syn::parse_quote!(
                        for<'__co3_borrow> <#ty as #co3::borrow::Borrow>::Borrowed<'__co3_borrow>:
                            #co3::Encode<
                                CType = <<#ty as #co3::ExternC>::CType as #co3::borrow::BorrowCast>::AsConst,
                                Store: #co3::stored::EmptyStore,
                            >
                    )
                };
                where_clause.predicates.push(bound);
            }
        }
    }

    if let syn::ReturnType::Type(_, output_ty) = &sig.output
        && detector.type_mentions_param(output_ty)
    {
        where_clause.predicates.push(syn::parse_quote!(
            for<'a> #output_ty: #co3::Decode<'a, Store: #co3::stored::EmptyStore>
        ));
    }

    wrapper_sig
}

fn dispatch_id_assignments(sig: &syn::Signature) -> Vec<TokenStream> {
    sig.inputs
        .iter()
        .filter_map(|input| {
            let FnArg::Typed(syn::PatType { pat, ty, .. }) = input else {
                return None;
            };
            let crate::dispatch::HandleId::DynType(ident) = crate::dispatch::handle_id(ty)? else {
                return None;
            };
            Some(quote! { let #pat = <#ident as co3::handle::Handle>::ID; })
        })
        .collect()
}

struct DispatchImportParts {
    set: TokenStream,
    id_checks: TokenStream,
    layout_checks: TokenStream,
    wrapper_sig: syn::Signature,
    id_assignments: Vec<TokenStream>,
    wrapper_body: TokenStream,
    extern_decl: TokenStream,
}

fn gen_dispatch_import_wrapper_body(
    DispatchImportParts {
        layout_checks,
        id_assignments,
        wrapper_body,
        extern_decl,
        ..
    }: &DispatchImportParts,
    self_binding: TokenStream,
) -> TokenStream {
    let co3 = co3_path();

    quote! {
        use #co3 as co3;
        #layout_checks
        #extern_decl
        #(#id_assignments)*
        #self_binding
        #wrapper_body
    }
}

fn gen_dispatch_import_module(
    module_name: &syn::Ident,
    DispatchImportParts { set, id_checks, .. }: &DispatchImportParts,
    definitions: TokenStream,
) -> TokenStream {
    quote! {
        #[allow(non_snake_case)]
        mod #module_name {
            #[allow(unused_imports)]
            use super::*;

            #[allow(non_camel_case_types)]
            #set
            #id_checks

            #definitions
        }
    }
}

#[expect(clippy::too_many_arguments)]
fn prepare_dispatch_import(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    block_attrs: &[syn::Attribute],
    fn_attrs: &[syn::Attribute],
    sig: &mut syn::Signature,
    dispatch_args: &mut DispatchGroups,
    set_name: &syn::Ident,
    self_ty: Option<&syn::Type>,
    impl_generics: Option<&syn::Generics>,
) -> DispatchImportParts {
    dispatch_args.inject_unnamed_lifetimes(&mut sig.generics);
    let mut dispatch_generics = sig.generics.clone();
    if let Some(impl_generics) = impl_generics {
        merge_generics(impl_generics.clone(), &mut dispatch_generics);
    }
    let co3 = co3_path();

    let wrapper_sig = prepare_dispatch_wrapper_sig(
        sig,
        &dispatch_generics,
        set_name,
        &co3,
        dispatch_type_idents(&dispatch_generics)
            .into_iter()
            .map(|ident| syn::parse_quote!(#ident)),
    );
    let id_assignments = dispatch_id_assignments(sig);
    let dummy_self_ty = syn::parse_quote!(());
    let wrapper_body = gen_wrapper_body::<true>(
        failure_mode,
        fn_attrs.iter().any(ffi_fn::is_by_val_attr),
        Some(self_ty.unwrap_or(&dummy_self_ty)),
        Some(&dispatch_generics),
        sig,
    );

    let mut extern_sig = sig.clone();
    normalize_fn_signature(&mut extern_sig, self_ty);
    if let Some(impl_generics) = impl_generics {
        merge_generics(impl_generics.clone(), &mut extern_sig.generics);
    }
    strip_erased_param_predicates(&dispatch_generics, &mut extern_sig.generics);
    let receiver = self_ty.map_or(crate::dispatch::DispatchReceiver::None, |ty| {
        crate::dispatch::DispatchReceiver::for_impl(ty, ty, None)
    });
    erase_handle_types(&dispatch_generics, receiver, &mut extern_sig);
    strip_dispatch_params(&mut extern_sig.generics);
    let decl = gen_extern_fn_signature(extern_sig, failure_mode);

    DispatchImportParts {
        set: gen_dispatch_set(
            set_name,
            &dispatch_generics,
            dispatch_args,
            dispatch_type_idents(&dispatch_generics)
                .into_iter()
                .map(|ident| syn::parse_quote!(#ident)),
        ),
        id_checks: gen_handle_id_uniqueness_checks(&dispatch_generics, dispatch_args),
        layout_checks: gen_dispatch_erased_layout_checks(&dispatch_generics, sig, dispatch_args),
        wrapper_sig,
        id_assignments,
        wrapper_body,
        extern_decl: gen_extern_decl(abi, block_attrs, fn_attrs, decl),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OwnershipMode {
    #[default]
    Borrow,
    ByValue,
}

pub(crate) fn expand_export_decls(
    abi: syn::Abi,
    _features: MacroFeatures,
    failure_mode: FailureMode,
    decls: Vec<ForeignItem>,
) -> TokenStream {
    let co3 = co3_path();

    let exports = decls.into_iter().map(|decl| {
        let export = match decl {
        ForeignItem::Static(item) => return crate::statics::gen_export_static(item),
        ForeignItem::Type(ForeignItemType {
            ty,
            id,
            self_impls,
            drop,
        }) => {
            let type_cfg_attrs = cfg_attrs(&ty.attrs).map(|attr| quote!(#attr)).collect::<Vec<_>>();
            let (impl_generics, ty_generics, where_clause) = ty.generics.split_for_impl();
            let mut decode_generics = ty.generics.clone();
            decode_generics.params.insert(0, syn::parse_quote!('_dšč));
            let (decode_impl_generics, _, _) = decode_generics.split_for_impl();

            let ident = &ty.ident;
            let dispatch = self_impls.into_iter().map(|impl_| {
                gen_export_impl(&abi, failure_mode, impl_, id.as_deref())
            });

            let drop_impl = drop.as_ref().map(|drop| {
                let mut item = drop.item.clone();
                if is_dyn_self_dispatch_for(&item, ident) {
                    crate::materialize_dyn_self_dispatch(&mut item);
                }
                item
            });

            let drop_check = gen_drop_impl_check(&ty, &drop_impl.unwrap());
            let drop = drop.map(|item| {
                if item.dispatch_args.is_empty() {
                    return gen_drop_impl_definition(&abi, failure_mode, item.item);
                }

                let self_id = dispatches_self(&item).then_some(id.as_deref()).flatten();
                gen_dispatch_export(&abi, failure_mode, item, self_id)
            });

            let opaque = derive_opaque_item(
                id.as_deref(),
                ident,
                &ty.generics,
                // TODO: This is not correct, but I don't think it matters whether it's ZST or not
                quote! { co3::rust_spec::size::Sized<co3::rust_spec::Gt<rust_spec::Zero>> },
                quote! { co3::rust_spec::niche::WithoutNiche },
            );
            let size_check = gen_non_zst_sized_check(ident, &ty.generics);

            quote! {
                #(#type_cfg_attrs)*
                const _: () = {
                    #opaque

                    #drop
                    #size_check
                    #drop_check

                    unsafe impl #impl_generics co3::stored::EncodeOwned for #ident #ty_generics #where_clause {
                        type Store = ();

                        #[inline(always)]
                        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
                        where
                            Self: 'itm
                        {
                            self
                        }
                    }
                    unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'_dšč> for #ident #ty_generics #where_clause {
                        type Store = ();

                        #[inline(always)]
                        unsafe fn soft_decode<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Option<Self> {
                            Some(source)
                        }
                    }

                    impl #impl_generics co3::Encode for #ident #ty_generics #where_clause {}
                    impl #impl_generics co3::Decode<'_> for #ident #ty_generics #where_clause {}

                    unsafe impl #impl_generics co3::borrow::BorrowCast for #ident #ty_generics #where_clause {
                        type AsConst = Self;
                    }
                    unsafe impl #impl_generics co3::borrow::BorrowCastMut for #ident #ty_generics #where_clause {
                        type AsMut = Self;
                    }

                    #(#dispatch)*
                };
            }
        }
        ForeignItem::Fn(item) => {
            if !item.dispatch_args.is_empty() {
                gen_dispatch_fn_export(&abi, failure_mode, item)
            } else {
                ffi_fn::gen_fn_definition(&abi, failure_mode, item.item)
            }
        }
        ForeignItem::Impl(impl_) => gen_export_impl(&abi, failure_mode, impl_, None),
    };

        quote! { const _: () = { use #co3 as co3; #export }; }
    });

    quote! { #(#exports)* }
}

pub(crate) fn expand_extern_decls(
    abi: syn::Abi,
    features: MacroFeatures,
    failure_mode: FailureMode,
    attrs: &[syn::Attribute],
    decls: Vec<ForeignItem>,
) -> TokenStream {
    fn gen_impl_extern_fn_decls(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        impl_: ItemImpl,
        self_id: Option<&syn::Type>,
        original_self_ty: Option<&syn::Type>,
        args: Option<&DispatchGroups>,
    ) -> Vec<TokenStream> {
        let receiver = crate::dispatch::DispatchReceiver::for_impl(
            original_self_ty.unwrap_or(&impl_.self_ty),
            &impl_.self_ty,
            self_id,
        );
        impl_
            .items
            .into_iter()
            .filter_map(|item| {
                let syn::ImplItem::Fn(mut item) = item else {
                    return None;
                };

                normalize_fn_signature(&mut item.sig, Some(&impl_.self_ty));
                merge_generics(impl_.generics.clone(), &mut item.sig.generics);

                let erased_layout_checks = args.map(|args| {
                    gen_dispatch_erased_layout_checks(&impl_.generics, &item.sig, args)
                });

                if args.is_some() {
                    strip_erased_param_predicates(&impl_.generics, &mut item.sig.generics);
                    erase_handle_types(&impl_.generics, receiver, &mut item.sig);
                    strip_dispatch_params(&mut item.sig.generics);
                }

                let decl = gen_extern_fn_signature(item.sig, failure_mode);
                let extern_decl = gen_extern_decl(abi, attrs, &item.attrs, decl);

                Some(quote! {
                    #erased_layout_checks
                    #extern_decl
                })
            })
            .collect()
    }

    fn expand_impl_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        impl_: ItemImpl,
    ) -> TokenStream {
        let co3 = co3_path();
        let import = wrap_impl_definition::<false>(failure_mode, &impl_);
        let extern_decl =
            gen_impl_extern_fn_decls(abi, failure_mode, attrs, impl_, None, None, None);

        quote! {
            const _: () = {
                use #co3 as co3;
                #(#extern_decl)*
                #import
            };
        }
    }

    fn expand_extern_dispatch_impl(impl_: &ItemImpl, args: &DispatchGroups) -> Vec<TokenStream> {
        let mut imports = Vec::new();
        args.for_each_combination(|selections| {
            let mut monomorphized = impl_.clone();
            monomorphized.generics.params = core::mem::take(&mut monomorphized.generics.params)
                .into_iter()
                .filter(|param| matches!(param, syn::GenericParam::Lifetime(_)))
                .collect();

            DispatchMonomorphizer::for_dispatch_group(&impl_.generics, selections)
                .visit_item_impl_mut(&mut monomorphized);
            imports.push(quote!(#monomorphized));
        });
        imports
    }

    fn expand_dispatch_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        self_id: Option<&syn::Type>,
        mut dispatch: Co3Impl,
    ) -> TokenStream {
        let original_self_ty = (*dispatch.self_ty).clone();
        crate::materialize_dyn_self_dispatch(&mut dispatch.item);
        let Co3Impl {
            item: mut impl_,
            dispatch_args,
            ..
        } = dispatch;
        let mut args = dispatch_args;
        args.inject_unnamed_lifetimes(&mut impl_.generics);

        let wrapped = wrap_impl_definition::<true>(failure_mode, &impl_);
        let dispatch_helper = gen_dispatch_helper(&impl_.generics, &args);
        let imports = expand_extern_dispatch_impl(&wrapped, &args);
        let extern_decl = gen_impl_extern_fn_decls(
            abi,
            failure_mode,
            attrs,
            impl_,
            self_id,
            Some(&original_self_ty),
            Some(&args),
        );
        let co3 = co3_path();

        quote! {
            const _: () = {
                use #co3 as co3;
                #dispatch_helper

                #(#extern_decl)*
                #(#imports)*
            };
        }
    }

    fn expand_dispatch_fn_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        mut dispatch: Co3Fn,
    ) -> TokenStream {
        let mut args = core::mem::take(&mut dispatch.dispatch_args);
        let syn::ItemFn {
            attrs: fn_attrs,
            sig,
            vis,
            ..
        } = &mut *dispatch;
        let dispatch_set = dispatch_set_name(fn_attrs, &sig.ident);
        let parts = prepare_dispatch_import(
            abi,
            failure_mode,
            attrs,
            fn_attrs,
            sig,
            &mut args,
            &dispatch_set,
            None,
            None,
        );
        let wrapper_attrs = fn_attrs
            .iter()
            .filter(|attr| !crate::is_symbol_name_attr(attr) && !ffi_fn::is_by_val_attr(attr));
        let module_name = dispatch_module_name(fn_attrs, &sig.ident);
        let vis = &*vis;
        let wrapper_name = &parts.wrapper_sig.ident;
        let wrapper_sig = &parts.wrapper_sig;
        let wrapper_body = gen_dispatch_import_wrapper_body(&parts, TokenStream::new());
        let module = gen_dispatch_import_module(
            &module_name,
            &parts,
            quote! {
                #[allow(private_bounds)]
                pub #wrapper_sig {
                    #wrapper_body
                }
            },
        );

        quote! {
            #[allow(private_bounds)]
            #(#wrapper_attrs)*
            #vis use #module_name::#wrapper_name;

            #module
        }
    }

    fn expand_dispatch_method_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        mut dispatch: Co3Impl,
    ) -> TokenStream {
        let mut args = core::mem::take(&mut dispatch.dispatch_args);
        let syn::ImplItem::Fn(mut method) = dispatch.items.pop().unwrap() else {
            unreachable!()
        };
        let self_ty = &dispatch.self_ty;
        let dispatch_set = dispatch_set_name(&method.attrs, &method.sig.ident);
        let parts = prepare_dispatch_import(
            abi,
            failure_mode,
            attrs,
            &method.attrs,
            &mut method.sig,
            &mut args,
            &dispatch_set,
            Some(self_ty),
            Some(&dispatch.generics),
        );
        let wrapper_attrs = method
            .attrs
            .iter()
            .filter(|attr| !crate::is_symbol_name_attr(attr) && !ffi_fn::is_by_val_attr(attr));
        let module_name = dispatch_module_name(&method.attrs, &method.sig.ident);
        let vis = &method.vis;
        let self_binding = method
            .sig
            .inputs
            .iter()
            .any(|input| matches!(input, FnArg::Receiver(_)))
            .then(|| quote!(let __co3_self = self;));
        let ItemImpl {
            attrs: impl_attrs,
            defaultness,
            unsafety,
            self_ty,
            ..
        } = &*dispatch;
        let mut wrapper_impl_generics = dispatch.generics.clone();
        wrapper_impl_generics
            .type_params_mut()
            .for_each(strip_internal_generic_param);
        let (impl_generics, _, where_clause) = wrapper_impl_generics.split_for_impl();
        let wrapper_sig = &parts.wrapper_sig;
        let wrapper_body =
            gen_dispatch_import_wrapper_body(&parts, self_binding.unwrap_or_default());

        gen_dispatch_import_module(
            &module_name,
            &parts,
            quote! {
                #(#impl_attrs)*
                #defaultness #unsafety impl #impl_generics #self_ty #where_clause {
                    #[allow(private_bounds)]
                    #(#wrapper_attrs)*
                    #vis #wrapper_sig {
                        #wrapper_body
                    }
                }
            },
        )
    }

    fn expand_import_impl(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        impl_: Co3Impl,
        type_id: Option<&syn::Type>,
    ) -> TokenStream {
        let (plain, methods) = split_dyn_methods(impl_);
        let plain = if plain.items.is_empty() {
            TokenStream::new()
        } else if !plain.dispatch_args.is_empty() {
            let self_id = dispatches_self(&plain).then_some(type_id).flatten();
            expand_dispatch_import(abi, failure_mode, attrs, self_id, plain)
        } else {
            expand_impl_import(abi, failure_mode, attrs, plain.item)
        };
        let methods = methods
            .into_iter()
            .map(|method| expand_dispatch_method_import(abi, failure_mode, attrs, method));

        quote!(#plain #(#methods)*)
    }

    let imports = decls.into_iter().map(|decl| match decl {
        ForeignItem::Type(ForeignItemType {
            ty,
            id,
            self_impls,
            drop,
        }) => {
            let ident = ty.ident.clone();
            let type_cfg_attrs = cfg_attrs(&ty.attrs)
                .map(|attr| quote!(#attr))
                .collect::<Vec<_>>();
            let ty = wrap_extern_type_decl(&abi, features, attrs, id.as_deref(), ty);

            let dispatch = self_impls
                .into_iter()
                .map(|impl_| expand_import_impl(&abi, failure_mode, attrs, impl_, id.as_deref()));

            let drop = drop.map(|item| {
                if item.dispatch_args.is_empty() {
                    expand_impl_import(&abi, failure_mode, attrs, item.item)
                } else {
                    let id = id.as_deref().unwrap();
                    expand_dispatch_drop_import(&abi, attrs, item.item, id, &ident)
                }
            });

            quote! {
                #ty

                #(#type_cfg_attrs)*
                const _: () = {
                    #drop
                    #(#dispatch)*
                };
            }
        }
        ForeignItem::Fn(item) => {
            if !item.dispatch_args.is_empty() {
                expand_dispatch_fn_import(&abi, failure_mode, attrs, item)
            } else {
                wrap_fn_definition(&abi, failure_mode, attrs, item.item)
            }
        }
        ForeignItem::Impl(impl_) => expand_import_impl(&abi, failure_mode, attrs, impl_, None),
        ForeignItem::Static(item) => crate::statics::gen_extern_static(&abi, attrs, item),
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

fn gen_non_zst_sized_check(ident: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let static_ty_generics = (!generics.params.is_empty() && !has_non_lifetime_generics(generics))
        .then(|| {
            let args = generics.params.iter().map(|param| match param {
                syn::GenericParam::Lifetime(_) => quote! { 'static },
                syn::GenericParam::Type(_) | syn::GenericParam::Const(_) => unreachable!(),
            });

            quote! { <#(#args),*> }
        });

    let non_zst_check = (!has_non_lifetime_generics(generics)).then(|| {
        quote! { const _: () = assert!(core::mem::size_of::<#ident #static_ty_generics>() != 0); }
    });

    quote! {
        const _: () = {
            trait __Co3AssertSized: core::marker::Sized {}

            impl #impl_generics __Co3AssertSized for #ident #ty_generics #where_clause {}

            #non_zst_check
        };
    }
}

fn gen_dispatch_helper(generics: &syn::Generics, args: &DispatchGroups) -> Option<TokenStream> {
    let erased_params = generics
        .type_params()
        .filter(|p| p.attrs.iter().any(is_type_erased))
        .collect::<Vec<_>>();

    let erased_tys = erased_params.iter().map(|p| &p.ident);
    let erased_generics = erased_params.iter().map(|param| {
        let mut param = (*param).clone();
        strip_internal_generic_param(&mut param);
        quote!(#param)
    });

    let mut dispatch_checks = Vec::new();
    args.for_each_combination(|selections| {
        let erased_args = selections
            .iter()
            .flat_map(|selection| selection.params.iter().zip(&selection.target.args))
            .filter_map(|(param, arg)| {
                generics
                    .type_params()
                    .find(|generic| generic.ident == *param)
                    .filter(|generic| generic.attrs.iter().any(is_type_erased))?;
                let mut arg = arg.clone();
                StaticLifetimeNormalizer.visit_generic_argument_mut(&mut arg);
                Some(quote!(#arg))
            });

        dispatch_checks.push(quote! {
            const _: __Co3DispatchParams::<#(#erased_args),*> =
                __Co3DispatchParams(core::marker::PhantomData);
        });
    });
    let handle_id_checks = gen_handle_id_uniqueness_checks(generics, args);

    Some(quote! {
        // NOTE: Verifies `?Sized` bounds on erased params
        struct __Co3DispatchParams<#(#erased_generics),*>(
            core::marker::PhantomData<(#(*const #erased_tys),*)>
        );

        #(#dispatch_checks)*
            #handle_id_checks
    })
}

fn gen_drop_impl_definition(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    mut impl_: ItemImpl,
) -> TokenStream {
    let self_ty = &impl_.self_ty;

    let decode_error = ffi_fn::gen_decode_error(failure_mode);
    let Some(syn::ImplItem::Fn(mut item)) = impl_.items.pop() else {
        unreachable!()
    };

    let ffi_fn_body = quote! {{
        let Some(__co3_self) = (unsafe {
            co3::decode(__co3_self)
        }) else {
            #decode_error
        };

        let __co3_self: &mut #self_ty = __co3_self;
        unsafe { core::ptr::drop_in_place(__co3_self as *mut _) };

        Ok::<_, ()>(())
    }};

    normalize_fn_signature(&mut item.sig, Some(self_ty));
    merge_generics(impl_.generics.clone(), &mut item.sig.generics);
    let fn_signature = gen_extern_fn_signature(item.sig, failure_mode);

    emit_extern_definition(abi, &item.attrs, failure_mode, fn_signature, ffi_fn_body)
}

fn expand_dispatch_drop_import(
    abi: &syn::Abi,
    attrs: &[syn::Attribute],
    mut impl_: ItemImpl,
    id_ty: &syn::Type,
    declared_type: &syn::Ident,
) -> TokenStream {
    if is_dyn_self_dispatch_for(&impl_, declared_type) {
        crate::materialize_dyn_self_dispatch(&mut impl_);
    }

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
        .find_map(|attr| symbol_name_value(attr).map(|value| quote!(#[link_name = #value])));
    let wrapper_attrs = wrapper_attrs
        .iter()
        .filter(|attr| symbol_name_value(attr).is_none());

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
                    fn drop(#(#values: #inputs),*) where #(#bounds,)*;
                }

                let __co3_self = self as *mut #self_ty as *mut core::ffi::c_void;
                #(#handle_id_conversion_stmts)*
                unsafe { drop(#(#values),*) };
            }
        }
    }
}

fn gen_drop_impl_check(item: &syn::ForeignItemType, impl_: &ItemImpl) -> TokenStream {
    let ident = &item.ident;
    let self_ty = &impl_.self_ty;

    let mut impl_generics = impl_.generics.clone();
    impl_generics.type_params_mut().for_each(|param| {
        strip_internal_generic_param(param);
    });

    let item_attrs = impl_
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("erased"));

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
    abi: &syn::Abi,
    features: MacroFeatures,
    attrs: &[syn::Attribute],
    id: Option<&syn::Type>,
    mut type_: syn::ForeignItemType,
) -> TokenStream {
    if has_non_lifetime_generics(&type_.generics) {
        let co3 = co3_path();
        let ident = &type_.ident;
        let generics = &type_.generics;
        let (_, ty_generics, _) = generics.split_for_impl();
        let extern_type = quote! { #ident #ty_generics };
        type_
            .generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote! { #extern_type: #co3::handle::Handle });
    }

    let syn::ForeignItemType {
        attrs: type_attrs,
        generics,
        vis,
        ident,
        ..
    } = type_;
    let type_cfg_attrs = cfg_attrs(&type_attrs)
        .map(|attr| quote!(#attr))
        .collect::<Vec<_>>();

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

    let type_decl = if features.extern_types {
        quote! {
            unsafe #abi {
                #(#attrs)*

                #(#type_attrs)*
                #vis type #ident #impl_generics #where_clause;
            }
        }
    } else {
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
        }
    };

    let co3 = co3_path();

    quote! {
        #type_decl

        #(#type_cfg_attrs)*
        #[doc = #owned_doc]
        #[repr(transparent)]
        #vis struct #owned_ident #impl_generics (*mut #ident #ty_generics) #where_clause;

        #(#type_cfg_attrs)*
        #[doc(hidden)]
        #[repr(transparent)]
        #[doc = #owned_repr_c_doc]
        #vis struct #owned_repr_c_name #impl_generics (*mut #ident #ty_generics) #where_clause;

        #(#type_cfg_attrs)*
        impl #impl_generics Drop for #owned_ident #ty_generics #where_clause {
            fn drop(&mut self) {
                unsafe { core::ptr::drop_in_place(self.0) }
            }
        }

        #(#type_cfg_attrs)*
        const _: () = {
            use #co3 as co3;

            #ident_impls
            #owned_impls
            #owned_repr_c_impls
        };
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
    let opaque_impls = derive_opaque_item(
        id,
        ident,
        generics,
        quote! { co3::rust_spec::size::ExternTypeLike },
        quote! { co3::rust_spec::niche::WithoutNiche },
    );

    quote! {
        #opaque_impls

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
    let mut decode_generics = generics.clone();
    decode_generics.params.insert(0, syn::parse_quote!('d));
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();
    let owned_repr_c_name = gen_owned_repr_c_name(ident);

    quote! {
        impl #impl_generics #owned_repr_c_name #ty_generics #where_clause {
            fn is_none(&self) -> bool {
                self.0.is_null()
            }
        }

        impl #impl_generics Clone for #owned_repr_c_name #ty_generics #where_clause {
            fn clone(&self) -> Self { *self }
        }
        impl #impl_generics Copy for #owned_repr_c_name #ty_generics #where_clause {}

        unsafe impl #impl_generics co3::rust_spec::RustSpec for #owned_repr_c_name #ty_generics #where_clause {
            type Layout = co3::rust_spec::Stable;
            type Size = co3::rust_spec::size::Sized<co3::rust_spec::Gt<rust_spec::Zero>>;
            type Alignment = <usize as co3::rust_spec::RustSpec>::Alignment;
            type Trap = co3::rust_spec::layout::Robust;
            type Niche = co3::rust_spec::niche::WithoutNiche;
            type Mutability = co3::rust_spec::mutability::Exclusive;
            type __IndirectTrap = co3::rust_spec::layout::Robust;
        }

        impl #impl_generics co3::ExternC for #owned_repr_c_name #ty_generics #where_clause {
            type CType = Self;
        }
        unsafe impl #impl_generics co3::stored::EncodeOwned for #owned_repr_c_name #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'d> for #owned_repr_c_name #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl #impl_generics co3::Encode for #owned_repr_c_name #ty_generics #where_clause {}
        impl #impl_generics co3::Decode<'_> for #owned_repr_c_name #ty_generics #where_clause {}

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #owned_repr_c_name #ty_generics #where_clause {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl #impl_generics co3::ReprC for #owned_repr_c_name #ty_generics #where_clause {}
        unsafe impl #impl_generics co3::CFnArg for #owned_repr_c_name #ty_generics #where_clause {}

        unsafe impl #impl_generics co3::borrow::BorrowCast for #owned_repr_c_name #ty_generics #where_clause {
            type AsConst = *const #ident #ty_generics;
        }
        unsafe impl #impl_generics co3::borrow::BorrowCastMut for #owned_repr_c_name #ty_generics #where_clause {
            type AsMut = *mut #ident #ty_generics;
        }
    }
}

fn gen_owned_extern_type_impls(ident: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut decode_generics = generics.clone();
    decode_generics.params.insert(0, syn::parse_quote!('d));
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();

    let owned_ident = gen_owned_extern_type_name(ident);
    let owned_repr_c_name = gen_owned_repr_c_name(ident);

    quote! {
        unsafe impl #impl_generics co3::rust_spec::RustSpec for #owned_ident #ty_generics #where_clause {
            type Layout = co3::rust_spec::Stable;
            type Size = co3::rust_spec::size::Sized<co3::rust_spec::Gt<rust_spec::Zero>>;
            type Alignment = <usize as co3::rust_spec::RustSpec>::Alignment;
            type Trap = co3::rust_spec::layout::NonRobust;
            type Niche = co3::rust_spec::niche::WithNiche<co3::rust_spec::Stable>;
            type Mutability = co3::rust_spec::mutability::Exclusive;
            type __IndirectTrap = co3::rust_spec::layout::Robust;
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

        unsafe impl #impl_generics co3::stored::EncodeOwned for #owned_ident #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                #owned_repr_c_name(core::mem::ManuallyDrop::new(self).0)
            }
        }
        unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'d> for #owned_ident #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                unsafe { <Self as co3::transmute::CheckedTransmute>::is_valid(&source) }.then_some(Self(source.0))
            }
        }

        impl #impl_generics co3::Encode for #owned_ident #ty_generics #where_clause {}
        impl #impl_generics co3::Decode<'_> for #owned_ident #ty_generics #where_clause {}

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
    size_kind: TokenStream,
    niche_kind: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let handle_family_impl = id.map(|id| gen_handle_family_impl(ident, generics, id));

    quote! {
        #handle_family_impl

        unsafe impl #impl_generics co3::rust_spec::RustSpec for #ident #ty_generics #where_clause {
            type Layout = co3::rust_spec::Stable;
            type Size = #size_kind;
            // TODO: This is just useless IMO
            type Alignment = co3::rust_spec::One;
            type Trap = co3::rust_spec::layout::Robust;
            type Niche = #niche_kind;
            type Mutability = co3::rust_spec::mutability::Exclusive;
            type __IndirectTrap = co3::rust_spec::layout::Robust;
        }

        unsafe impl #impl_generics co3::ReprC for #ident #ty_generics #where_clause {}

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #ident #ty_generics #where_clause {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        impl #impl_generics co3::ExternC for #ident #ty_generics #where_clause {
            type CType = Self;
        }
    }
}

fn strip_erased_param_predicates(impl_generics: &syn::Generics, sig_generics: &mut syn::Generics) {
    let erased_params = impl_generics
        .type_params()
        .filter(|param| param.attrs.iter().any(is_type_erased))
        .map(|param| &param.ident)
        .collect::<BTreeSet<_>>();

    let Some(where_clause) = &mut sig_generics.where_clause else {
        return;
    };

    let old_predicates = core::mem::take(&mut where_clause.predicates);
    let detector = ParamUseDetector::new(erased_params.iter().copied());
    let mut new_predicates = Punctuated::new();

    for predicate in old_predicates {
        if !detector.predicate_mentions_param(&predicate) {
            new_predicates.push(predicate);
        }
    }

    where_clause.predicates = new_predicates;
}
