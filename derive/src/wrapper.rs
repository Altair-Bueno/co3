use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::{FnArg, ItemImpl, punctuated::Punctuated, visit_mut::VisitMut};

use crate::{
    dispatch::{
        HandleId, gen_handle_erase_stmts, gen_return_derase_expr, handle_id, is_handle_id_arg,
    },
    ffi_fn::{
        self, gen_return_borrow_check, gen_soft_sync_error_value, gen_trap_value, is_by_val_attr,
        is_spread_arg, item_fn_input_ident, ownership_mode_for_arg, spread_arg_names,
    },
    generate::OwnershipMode,
    is_symbol_name_attr,
    parse::FailureMode,
    symbol_name_value,
    utils::{co3_path, gen_store_name, is_drop_impl, soft_for_arg, strip_internal_generic_param},
};

pub(crate) fn strip_internal_arg_attrs(signature: &mut syn::Signature) {
    struct InternalAttrStripper;

    impl VisitMut for InternalAttrStripper {
        fn visit_receiver_mut(&mut self, node: &mut syn::Receiver) {
            node.attrs.retain(|attr| {
                !is_by_val_attr(attr)
                    && !attr.path().is_ident("soft")
                    && !attr.path().is_ident("spread")
                    && !attr.path().is_ident("try_spread")
            });
        }

        fn visit_pat_type_mut(&mut self, node: &mut syn::PatType) {
            node.attrs.retain(|attr| {
                !is_by_val_attr(attr)
                    && !attr.path().is_ident("soft")
                    && !attr.path().is_ident("spread")
                    && !attr.path().is_ident("try_spread")
            });
        }
    }

    InternalAttrStripper.visit_signature_mut(signature);
}

pub(crate) fn wrap_fn_definition(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    block_attrs: &[syn::Attribute],
    mut item: syn::ItemFn,
) -> TokenStream {
    let vis = &item.vis;

    let wrapper_attrs = item
        .attrs
        .iter()
        .filter(|attr| !is_symbol_name_attr(attr) && !is_by_val_attr(attr));

    let mut wrapper_sig = item.sig.clone();
    strip_internal_arg_attrs(&mut wrapper_sig);

    let wrapper_body = gen_wrapper_body::<false>(
        failure_mode,
        item.attrs.iter().any(is_by_val_attr),
        None,
        None,
        false,
        &item.sig,
    );
    let co3 = co3_path();

    ffi_fn::normalize_fn_signature(&mut item.sig, None);
    let decl = ffi_fn::gen_extern_fn_signature(item.sig, failure_mode);
    let extern_fn_decl = gen_extern_decl(abi, block_attrs, &item.attrs, decl);

    quote! {
        #(#wrapper_attrs)*
        #vis #wrapper_sig {
            use #co3 as co3;
            #extern_fn_decl
            #wrapper_body
        }
    }
}

pub fn wrap_impl_definition<const DISPATCHED: bool>(
    failure_mode: FailureMode,
    impl_: &ItemImpl,
    erase_declared_receiver: bool,
) -> ItemImpl {
    let ItemImpl {
        attrs: impl_attrs,
        modifiers,
        unsafety,
        generics,
        trait_,
        self_ty,
        items,
        ..
    } = impl_;

    let trait_ = trait_.as_ref().map(|(path, _)| path);
    let drop_impl = is_drop_impl(impl_);
    let defaultness = &modifiers.defaultness;
    let methods = items.iter().map(|item| {
        let syn::ImplItem::Fn(item) = item else {
            return quote!(#item);
        };

        let mut sig = item.sig.clone();
        let vis = &item.vis;

        let wrapper_attrs = item
            .attrs
            .iter()
            .filter(|attr| !is_symbol_name_attr(attr) && !is_by_val_attr(attr));

        let mut wrapper_item = item.clone();
        if drop_impl && matches!(item.sig.output, syn::ReturnType::Type(_, _)) {
            sig.output = syn::ReturnType::Default;
            wrapper_item.sig.output = syn::ReturnType::Default;
        }
        let body = gen_impl_wrapper_body::<DISPATCHED>(
            failure_mode,
            &wrapper_item,
            self_ty,
            generics,
            erase_declared_receiver,
        );

        sig.inputs = if DISPATCHED {
            sig.inputs
                .into_iter()
                .filter(|i| !is_handle_id_arg(i))
                .collect()
        } else {
            core::mem::take(&mut sig.inputs)
        };

        if let Some(position) = sig
            .inputs
            .iter()
            .position(|input| matches!(input, FnArg::Receiver(_)))
            && position != 0
        {
            let mut inputs = core::mem::take(&mut sig.inputs)
                .into_iter()
                .collect::<Vec<_>>();
            let receiver = inputs.remove(position);
            sig.inputs = core::iter::once(receiver).chain(inputs).collect();
        }

        strip_internal_arg_attrs(&mut sig);

        quote! {
            #(#wrapper_attrs)*
            #vis #sig {
                #body
            }
        }
    });

    let generics = if DISPATCHED {
        let mut generics = generics.clone();

        generics.type_params_mut().for_each(|param| {
            strip_internal_generic_param(param);
        });

        generics
    } else {
        generics.clone()
    };

    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let impl_head = if let Some(trait_) = &trait_ {
        quote!(#trait_ for)
    } else {
        quote!()
    };

    syn::parse_quote! {
        #(#impl_attrs)*
        #defaultness #unsafety impl #impl_generics #impl_head #self_ty #where_clause {
            #(#methods)*
        }
    }
}

pub(crate) fn gen_impl_wrapper_body<const DISPATCHED: bool>(
    failure_mode: FailureMode,
    item: &syn::ImplItemFn,
    self_ty: &syn::Type,
    generics: &syn::Generics,
    erase_declared_receiver: bool,
) -> TokenStream {
    let self_binding = item
        .sig
        .inputs
        .iter()
        .any(|input| matches!(input, FnArg::Receiver(_)))
        .then(|| quote! { let __co3_self = self; });

    let id_assignments = item.sig.inputs.iter().filter_map(|input| {
        let FnArg::Typed(syn::PatType { pat, ty, .. }) = input else {
            return None;
        };

        let handle_ty = match handle_id(ty)? {
            HandleId::DynType(ty_param) => quote!(#ty_param),
            HandleId::DynSelf => quote!(#self_ty),
        };

        Some(quote! { let #pat = <#handle_ty as co3::handle::Handle>::ID; })
    });
    let wrapper_body = gen_wrapper_body::<DISPATCHED>(
        failure_mode,
        item.attrs.iter().any(is_by_val_attr),
        Some(self_ty),
        Some(generics),
        erase_declared_receiver,
        &item.sig,
    );
    let co3 = co3_path();

    quote! {
        use #co3 as co3;
        #(#id_assignments)*
        #self_binding
        #wrapper_body
    }
}

pub(crate) fn gen_extern_decl(
    abi: &syn::Abi,
    block_attrs: &[syn::Attribute],
    attrs: &[syn::Attribute],
    decl: TokenStream,
) -> TokenStream {
    let decl_attrs = attrs.iter().filter_map(|attr| {
        let value = symbol_name_value(attr)?;
        Some(quote!(#[link_name = #value]))
    });

    quote! {
        unsafe #abi {
            #(#block_attrs)*

            #(#decl_attrs)*
            #decl;
        }
    }
}

pub(crate) fn gen_wrapper_body<const DISPATCHED: bool>(
    failure_mode: FailureMode,
    fn_by_val: bool,
    self_ty: Option<&syn::Type>,
    dispatch_generics: Option<&syn::Generics>,
    erase_declared_receiver: bool,
    sig: &syn::Signature,
) -> TokenStream {
    let fn_name = &sig.ident;
    gen_wrapper_body_with_callee::<DISPATCHED>(
        failure_mode,
        fn_by_val,
        self_ty,
        dispatch_generics,
        erase_declared_receiver,
        sig,
        quote!(#fn_name),
    )
}

pub(crate) fn gen_wrapper_body_with_callee<const DISPATCHED: bool>(
    failure_mode: FailureMode,
    fn_by_val: bool,
    self_ty: Option<&syn::Type>,
    dispatch_generics: Option<&syn::Generics>,
    erase_declared_receiver: bool,
    sig: &syn::Signature,
    callee: TokenStream,
) -> TokenStream {
    let handle_erase_stmts = if DISPATCHED {
        self_ty
            .zip(dispatch_generics)
            .map(|(self_ty, generics)| {
                gen_handle_erase_stmts(self_ty, generics, erase_declared_receiver, sig)
            })
            .unwrap_or_default()
    } else {
        Default::default()
    };

    let input_convert = gen_input_conversion_stmts(&sig.inputs);
    let spread_inputs = gen_spread_input_stmts(failure_mode, &sig.inputs);
    let declared_receiver_erase = if !DISPATCHED
        && erase_declared_receiver
        && crate::dispatch::has_declared_self_input(self_ty.unwrap(), sig)
    {
        crate::dispatch::gen_dyn_self_erase_stmts(self_ty.unwrap(), sig)
    } else {
        Default::default()
    };
    let store_sync_stmts = gen_store_sync_stmts(&sig.inputs);
    let sync_check = gen_wrapper_sync_check(failure_mode, sig.inputs.len(), store_sync_stmts);

    let ffi_fn_call = gen_ffi_fn_call(sig, &callee);
    if let syn::ReturnType::Type(_, output_ty) = &sig.output {
        let return_derase = gen_return_derase::<DISPATCHED>(self_ty, dispatch_generics, output_ty);
        let return_borrow_check = gen_return_borrow_check(output_ty, fn_by_val);
        let decode_error = gen_return_decode_error(failure_mode, output_ty);

        return quote! {
            #return_borrow_check

            #input_convert
            #(#declared_receiver_erase)*
            let __co3_out = {
                #(#handle_erase_stmts)*
                #spread_inputs
                #ffi_fn_call
            };

            #sync_check
            let __co3_out = #return_derase;
            let __co3_out: Option<#output_ty> = unsafe {
                co3::decode(__co3_out)
            };

            let Some(__co3_out) = __co3_out else {
                #decode_error
            };

            __co3_out
        };
    }

    quote! {
        #input_convert
        #(#declared_receiver_erase)*

        {
            #(#handle_erase_stmts)*
            #spread_inputs
            #ffi_fn_call;
        }

        #sync_check
    }
}

fn gen_wrapper_sync_check(
    failure_mode: FailureMode,
    inputs_len: usize,
    store_sync_stmts: TokenStream,
) -> TokenStream {
    let sync_error = match failure_mode {
        FailureMode::Panic => {
            let sync_success_args = (0..inputs_len)
                .map(|idx| {
                    let idx = syn::Index::from(idx);
                    quote! { u8::from(!__co3_sync_errors[#idx]) }
                })
                .collect::<Vec<_>>();

            let sync_success_fmt = if sync_success_args.is_empty() {
                "\n".to_owned()
            } else {
                let placeholders = core::iter::repeat_n("{}", sync_success_args.len())
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("\nArg Sync: ({placeholders})\n")
            };

            if sync_success_args.is_empty() {
                quote! { panic!(#sync_success_fmt); }
            } else {
                quote! { panic!(#sync_success_fmt, #(#sync_success_args),*); }
            }
        }
        FailureMode::Error => {
            let error = gen_soft_sync_error_value();
            quote! { return #error; }
        }
    };

    ffi_fn::gen_sync_check(store_sync_stmts, sync_error)
}

fn gen_return_decode_error(failure_mode: FailureMode, output_ty: &syn::Type) -> TokenStream {
    match failure_mode {
        FailureMode::Panic => quote! {
            panic!(concat!(stringify!(#output_ty), "Decode failed"));
        },
        FailureMode::Error => {
            let error = gen_trap_value();
            quote! { return #error; }
        }
    }
}

fn gen_return_derase<const DISPATCHED: bool>(
    self_ty: Option<&syn::Type>,
    dispatch_generics: Option<&syn::Generics>,
    output_ty: &syn::Type,
) -> TokenStream {
    if !DISPATCHED {
        return quote!(__co3_out);
    }

    self_ty
        .zip(dispatch_generics)
        .map(|(self_ty, generics)| {
            let mut output_ty = output_ty.clone();
            ffi_fn::SelfConcretizer { self_ty }.visit_type_mut(&mut output_ty);
            gen_return_derase_expr(generics, &output_ty, quote!(__co3_out))
        })
        .unwrap_or_else(|| quote!(__co3_out))
}

fn gen_store_sync_stmts(inputs: &Punctuated<FnArg, syn::Token![,]>) -> TokenStream {
    let input_len = inputs.len();
    let mut store_sync_stmts = quote! {};

    for (idx, input) in inputs.iter().enumerate() {
        let (attrs, arg_name) = match input {
            FnArg::Typed(arg) => (&arg.attrs, item_fn_input_ident(&arg.pat).clone()),
            FnArg::Receiver(receiver) => (&receiver.attrs, format_ident!("__co3_self")),
        };

        if soft_for_arg(attrs) {
            let store_name = gen_store_name(&arg_name);

            store_sync_stmts.extend(quote! {
                if co3::stored::Store::sync(#store_name).is_none() {
                    __co3_sync_errors[#idx] = true;
                }
            });
        }
    }

    quote! {{
        let mut __co3_sync_errors = [false; #input_len];

        #store_sync_stmts
        __co3_sync_errors
    }}
}

fn gen_input_conversion_stmts(inputs: &Punctuated<FnArg, syn::Token![,]>) -> TokenStream {
    let mut stmts = quote! {};

    for input in inputs {
        let (attrs, arg_name) = match input {
            FnArg::Typed(syn::PatType { attrs, pat, .. }) => {
                (attrs, item_fn_input_ident(pat).clone())
            }
            FnArg::Receiver(receiver) => (&receiver.attrs, format_ident!("__co3_self")),
        };

        let store_name = gen_store_name(&arg_name);
        if OwnershipMode::Borrow == ownership_mode_for_arg(attrs) {
            let owner_name = format_ident!("__co3_{arg_name}_owner");

            stmts.extend(quote! {
                let mut #owner_name = Default::default();

                let #arg_name = co3::borrow::Borrow::borrow(
                    #arg_name, &mut #owner_name
                );
            });
        }

        stmts.extend(if soft_for_arg(attrs) {
            quote! {
                let mut #store_name = Default::default();

                let #arg_name = co3::soft_encode(
                    #arg_name, &mut #store_name
                );
            }
        } else {
            quote! { let #arg_name = co3::encode(#arg_name); }
        });
    }

    stmts
}

fn gen_spread_input_stmts(
    failure_mode: FailureMode,
    inputs: &Punctuated<FnArg, syn::Token![,]>,
) -> TokenStream {
    let stmts = inputs.iter().filter_map(|input| {
        let FnArg::Typed(syn::PatType { attrs, pat, .. }) = input else {
            return None;
        };
        if !is_spread_arg(attrs) {
            return None;
        }
        let arg_name = item_fn_input_ident(pat);
        let (data_name, metadata_name) = spread_arg_names(arg_name);
        let (target1_ty, target2_ty) = ffi_fn::spread_types(attrs)
            .expect("validated #[spread] attribute")
            .unwrap();
        let try_spread = ffi_fn::is_try_spread_arg(attrs);
        let data_convert =
            spread_part_conversion(&data_name, &target1_ty, try_spread, failure_mode);
        let metadata_convert =
            spread_part_conversion(&metadata_name, &target2_ty, try_spread, failure_mode);
        Some(quote! {
            let (#data_name, #metadata_name) = co3::slice::Spread2::into_parts(#arg_name);
            let #data_name = #data_convert;
            let #metadata_name = #metadata_convert;
        })
    });

    quote!(#(#stmts)*)
}

fn spread_part_conversion(
    name: &Ident,
    target: &syn::Type,
    try_spread: bool,
    failure_mode: FailureMode,
) -> TokenStream {
    if matches!(target, syn::Type::Infer(_)) {
        return quote!(#name);
    }

    let target_abi_ty: syn::Type = syn::parse_quote!(<#target as co3::ExternC>::CType);
    if try_spread {
        match failure_mode {
            FailureMode::Panic => quote! {
                core::convert::TryInto::<#target_abi_ty>::try_into(#name)
                    .unwrap_or_else(|_| panic!("co3 generated FFI spread conversion failure"))
            },
            FailureMode::Error => quote! {
                core::convert::TryInto::<#target_abi_ty>::try_into(#name)
                    .map_err(|_| co3::Error::trap_value())?
            },
        }
    } else {
        quote!(core::convert::Into::<#target_abi_ty>::into(#name))
    }
}

fn gen_ffi_fn_call(sig: &syn::Signature, callee: &TokenStream) -> TokenStream {
    let arg_names = sig.inputs.iter().map(|input| match input {
        FnArg::Receiver(_) => quote!(__co3_self),
        FnArg::Typed(syn::PatType { attrs, pat, .. }) => {
            let arg_name = item_fn_input_ident(pat);
            if is_spread_arg(attrs) {
                let (data_name, metadata_name) = spread_arg_names(arg_name);
                quote!(#data_name, #metadata_name)
            } else {
                quote!(#arg_name)
            }
        }
    });

    quote! {
        unsafe { #callee(#(#arg_names),*) }
    }
}
