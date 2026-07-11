use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    GenericArgument, GenericParam, Result, parse::Parser, parse_quote, punctuated::Punctuated,
    spanned::Spanned, visit::Visit, visit_mut::VisitMut,
};

use crate::{
    DynImpl,
    ffi_fn::{
        emit_extern_definition, gen_definition_body, gen_extern_fn_signature,
        gen_fn_signature_check, gen_input_decode_stmts, gen_store_sync_stmts, is_spread_arg,
        item_fn_input_arg_type, merge_generics, normalize_fn_signature, output_abi_ty,
        strip_erased_type_params,
    },
    utils::{
        DispatchMonomorphizer, ParamUseDetector, erased_abi_repr, erased_id_repr, is_drop_impl,
        is_type_erased, unwrap_result_type,
    },
};

#[derive(Clone, Copy)]
enum RetypeDirection {
    Erase,
    Derase,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HandleId<'a> {
    DynType(&'a syn::Ident),
    DynSelf,
}

struct ErasedParamReplacer {
    erased_params: BTreeMap<syn::Ident, syn::Type>,
}

impl ErasedParamReplacer {
    fn new(generics: &syn::Generics) -> Self {
        Self {
            erased_params: generics
                .type_params()
                .filter(|p| p.attrs.iter().any(is_type_erased))
                .map(|p| (p.ident.clone(), erased_abi_repr(p)))
                .collect(),
        }
    }

    fn replace(&mut self, mut ty: syn::Type) -> syn::Type {
        self.visit_type_mut(&mut ty);
        ty
    }
}

impl VisitMut for ErasedParamReplacer {
    fn visit_type_mut(&mut self, node: &mut syn::Type) {
        if let syn::Type::Path(syn::TypePath {
            qself: None, path, ..
        }) = node
            && let Some(repr) = path.get_ident().and_then(|i| self.erased_params.get(i))
        {
            *node = repr.clone();
            return;
        }

        syn::visit_mut::visit_type_mut(self, node);
    }
}

pub(crate) fn find_dispatch_attr(attrs: &[syn::Attribute]) -> Option<&syn::Attribute> {
    attrs.iter().find(|&attr| attr.path().is_ident("dispatch"))
}

pub(crate) fn parse_dispatch_attr(
    impl_: &syn::ItemImpl,
    allow_empty: bool,
) -> Result<Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>> {
    let Some(attr) = find_dispatch_attr(&impl_.attrs) else {
        return Ok(Punctuated::default());
    };

    let params = &impl_
        .generics
        .params
        .iter()
        .filter(|arg| !matches!(arg, GenericParam::Lifetime(_)))
        .collect::<Vec<_>>();

    let err_msg = format!(
        "dispatch must provide {} generic argument{}",
        params.len(),
        if params.len() == 1 { "" } else { "s" }
    );

    let syn::Meta::List(list) = &attr.meta else {
        if !allow_empty && !params.is_empty() {
            return Err(syn::Error::new_spanned(attr, err_msg));
        }

        return Ok(Punctuated::default());
    };

    let mut generic_args: Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]> =
        Punctuated::parse_terminated.parse2(list.tokens.clone())?;

    if generic_args.is_empty() && !allow_empty && !params.is_empty() {
        return Err(syn::Error::new_spanned(attr, err_msg));
    }

    struct LifetimeArgValidator {
        errors: Option<syn::Error>,
    }

    impl LifetimeArgValidator {
        fn push(&mut self, err: syn::Error) {
            if let Some(errors) = &mut self.errors {
                errors.combine(err);
            } else {
                self.errors = Some(err);
            }
        }
    }

    impl Visit<'_> for LifetimeArgValidator {
        fn visit_lifetime(&mut self, node: &syn::Lifetime) {
            if node.ident == "_" {
                return;
            }

            let err_msg = "undeclared lifetime; consider using '_";
            self.push(syn::Error::new_spanned(node, err_msg));
        }
    }

    let mut lifetime_validator = LifetimeArgValidator { errors: None };

    for entry in &generic_args {
        for arg in &entry.args {
            if matches!(arg, syn::GenericArgument::Lifetime(_)) {
                let err_msg = "lifetime arguments not required in #[dispatch]";
                lifetime_validator.push(syn::Error::new_spanned(arg, err_msg));
                continue;
            }
            lifetime_validator.visit_generic_argument(arg);
        }
    }

    if let Some(errors) = lifetime_validator.errors {
        return Err(errors);
    }

    for entry in &generic_args {
        let args_len = entry
            .args
            .iter()
            .filter(|arg| !matches!(arg, GenericArgument::Lifetime(_)))
            .count();

        if args_len != params.len() {
            return Err(syn::Error::new_spanned(entry, err_msg));
        }
    }

    let mut errors = None::<syn::Error>;
    for entry in &mut generic_args {
        let err_msg = "argument kind must match declared parameter kind";

        for (param, arg) in params.iter().zip(
            entry
                .args
                .iter_mut()
                .filter(|arg| !matches!(arg, GenericArgument::Lifetime(_))),
        ) {
            if let (GenericParam::Const(_), GenericArgument::Type(ty)) = (param, &arg)
                && matches!(ty, syn::Type::Path(_))
            {
                *arg = parse_quote!({ #ty });
            }
        }

        for (param, arg) in params.iter().zip(
            entry
                .args
                .iter()
                .filter(|arg| !matches!(arg, GenericArgument::Lifetime(_))),
        ) {
            let mismatch = match param {
                GenericParam::Lifetime(_) => false,
                GenericParam::Type(_) => !matches!(arg, GenericArgument::Type(_)),
                GenericParam::Const(_) => !matches!(arg, GenericArgument::Const(_)),
            };

            if mismatch {
                let err = syn::Error::new_spanned(arg, err_msg);

                if let Some(errors) = &mut errors {
                    errors.combine(err);
                } else {
                    errors = Some(err);
                }
            }
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(generic_args)
}

pub(crate) fn gen_dispatch_export(
    abi: &syn::Abi,
    dispatch: DynImpl,
    self_id: Option<&syn::Type>,
) -> TokenStream {
    let DynImpl { impl_, args } = dispatch;

    let trait_ = impl_.trait_.as_ref().map(|(_, path, _)| path);
    let self_ty = &impl_.self_ty;
    let generics = &impl_.generics;
    let dispatch_id_checks = gen_dispatch_id_uniqueness_checks(generics, &args);

    let drop_impl = is_drop_impl(&impl_);
    let items = impl_.items.into_iter().filter_map(|item| {
        let syn::ImplItem::Fn(item) = item else {
            return None;
        };

        Some(item)
    });

    let definitions = items.map(|mut item| {
        merge_generics(impl_.generics.clone(), &mut item.sig.generics);
        normalize_fn_signature(&mut item.sig, Some(self_ty));
        let erased_layout_checks = gen_dispatch_erased_layout_checks(generics, &item.sig, &args);
        monomorphize_predicates(&mut item.sig.generics, &args);
        strip_erased_type_params(&mut item.sig.generics);

        let (id_arg_names, handle_ids): (Vec<_>, Vec<_>) = item
            .sig
            .inputs
            .iter()
            .filter_map(|input| {
                let syn::FnArg::Typed(syn::PatType { pat, ty, .. }) = input else {
                    return None;
                };

                let handle_id = match handle_id(ty)? {
                    HandleId::DynSelf => self_id.cloned(),
                    HandleId::DynType(ident) => generics
                        .type_params()
                        .find(|param| param.ident == *ident)
                        .and_then(erased_id_repr),
                };

                Some((pat, parse_quote!(#pat: #handle_id)))
            })
            .unzip();

        let dispatch_arms =
            gen_dispatch_arms(generics, trait_, self_ty, &item.sig, drop_impl, &args)
                .collect::<Vec<_>>();

        let decode_id_stmts = gen_input_decode_stmts(&handle_ids);
        let sync_id_stores = gen_store_sync_stmts(handle_ids.len());

        let fn_body = quote! {{
            #decode_id_stmts

            let __co3_dispatch_result: Result<(), co3::FfiReturn> = match (#(#id_arg_names,)*) {
                #(#dispatch_arms,)*
                _ => Err(co3::FfiReturn::UnknownHandle),
            };

            let __co3_sync_errors = #sync_id_stores;
            let mut __co3_sync_errors_iter = core::iter::IntoIterator::into_iter(__co3_sync_errors);
            if core::iter::Iterator::any(&mut __co3_sync_errors_iter, core::convert::identity) {
                return Err(co3::FfiReturn::TrapRepresentation);
            }

            __co3_dispatch_result
        }};

        erase_handle_types(generics, self_id, &mut item.sig, &args);
        let sig = gen_extern_fn_signature(item.sig);
        let definition = emit_extern_definition(abi, &item.attrs, sig, fn_body);

        quote! {
            #erased_layout_checks
            #definition
        }
    });

    quote! {
        #dispatch_id_checks
        #(#definitions)*
    }
}

pub(crate) fn gen_dispatch_id_uniqueness_checks(
    generics: &syn::Generics,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) -> TokenStream {
    let checks = generics.type_params().filter_map(|param| {
        let repr = erased_id_repr(param)?;

        let enum_ident = format_ident!("DispatchIdCheck{}", param.ident);
        let variants = args.iter().enumerate().filter_map(|(entry_idx, entry)| {
            let variant = format_ident!("DispatchId{entry_idx}");

            let mut arg = generics
                .params
                .iter()
                .filter(|param| !matches!(param, GenericParam::Lifetime(_)))
                .zip(&entry.args)
                .find_map(|(generic_param, arg)| match generic_param {
                    GenericParam::Type(generic_param) if generic_param.ident == param.ident => {
                        Some(arg.clone())
                    }
                    _ => None,
                })?;
            StaticLifetimeNormalizer.visit_generic_argument_mut(&mut arg);

            Some(quote! { #variant = <#arg as co3::handle::Handle>::ID })
        });

        Some(quote! {
            const _: () = {
                #[repr(#repr)]
                enum #enum_ident {
                    #(#variants,)*
                }
            };
        })
    });

    quote! { #(#checks)* }
}

pub(crate) fn gen_dispatch_erased_layout_checks(
    generics: &syn::Generics,
    sig: &syn::Signature,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) -> TokenStream {
    let param_detector = ParamUseDetector::new(
        generics
            .type_params()
            .filter(|p| p.attrs.iter().any(is_type_erased))
            .map(|p| &p.ident),
    );

    let checks = args.iter().flat_map(|entry| {
        let mut checks = Vec::new();

        for input in &sig.inputs {
            let (attrs, ty) = match input {
                syn::FnArg::Receiver(receiver) => (&receiver.attrs[..], &*receiver.ty),
                syn::FnArg::Typed(syn::PatType { attrs, ty, .. }) => (&attrs[..], &**ty),
            };

            if handle_id(ty).is_some() || !param_detector.type_mentions_param(ty) {
                continue;
            }

            let concrete_tys = dispatch_input_abi_tys(generics, entry, attrs, ty);
            let erased_tys = erased_input_abi_tys(generics, attrs, ty);

            checks.extend(dispatch_layout_checks(
                concrete_tys.into_iter().zip(erased_tys),
            ));
        }

        let syn::ReturnType::Type(_, output_ty) = &sig.output else {
            return checks;
        };

        let output_ty = unwrap_result_type(output_ty)
            .map(|(ok, _)| ok)
            .unwrap_or(output_ty);

        if param_detector.type_mentions_param(output_ty) {
            let concrete_ty = dispatch_output_abi_ty(generics, entry, output_ty);
            let erased_ty = erased_output_abi_ty(generics, output_ty);

            checks.extend(dispatch_layout_checks([(concrete_ty, erased_ty)]));
        }

        checks
    });

    quote! { #(#checks)* }
}

fn dispatch_layout_checks(
    pairs: impl IntoIterator<Item = (syn::Type, syn::Type)>,
) -> Vec<TokenStream> {
    pairs
        .into_iter()
        .map(|(mut concrete_ty, mut erased_ty)| {
            StaticLifetimeNormalizer.visit_type_mut(&mut concrete_ty);
            StaticLifetimeNormalizer.visit_type_mut(&mut erased_ty);

            quote! {
                const {
                    assert!(
                        core::mem::size_of::<#concrete_ty>()
                            == core::mem::size_of::<#erased_ty>(),
                        "erased argument size mismatch",
                    );
                    assert!(
                        core::mem::align_of::<#concrete_ty>()
                            == core::mem::align_of::<#erased_ty>(),
                        "erased argument alignment mismatch",
                    );
                }
            }
        })
        .collect()
}

pub(crate) struct StaticLifetimeNormalizer;
impl VisitMut for StaticLifetimeNormalizer {
    fn visit_lifetime_mut(&mut self, node: &mut syn::Lifetime) {
        *node = syn::Lifetime::new("'static", node.span());
    }
}

fn dispatch_input_abi_tys(
    generics: &syn::Generics,
    entry: &syn::AngleBracketedGenericArguments,
    attrs: &[syn::Attribute],
    ty: &syn::Type,
) -> Vec<syn::Type> {
    let mut ty = ty.clone();
    DispatchMonomorphizer::new(generics, entry).visit_type_mut(&mut ty);
    input_abi_tys(attrs, &ty)
}

fn dispatch_output_abi_ty(
    generics: &syn::Generics,
    entry: &syn::AngleBracketedGenericArguments,
    ty: &syn::Type,
) -> syn::Type {
    let mut ty = ty.clone();
    DispatchMonomorphizer::new(generics, entry).visit_type_mut(&mut ty);
    output_abi_ty(&ty)
}

fn erased_input_abi_tys(
    generics: &syn::Generics,
    attrs: &[syn::Attribute],
    ty: &syn::Type,
) -> Vec<syn::Type> {
    let ty = ErasedParamReplacer::new(generics).replace(ty.clone());
    input_abi_tys(attrs, &ty)
}

fn erased_output_abi_ty(generics: &syn::Generics, ty: &syn::Type) -> syn::Type {
    let ty = ErasedParamReplacer::new(generics).replace(ty.clone());
    output_abi_ty(&ty)
}

fn input_abi_tys(attrs: &[syn::Attribute], ty: &syn::Type) -> Vec<syn::Type> {
    let abi_ty = item_fn_input_arg_type(attrs, ty);

    if is_spread_arg(attrs) {
        return vec![
            parse_quote!(<#abi_ty as co3::size::Spread>::Part1),
            parse_quote!(<#abi_ty as co3::size::Spread>::Part2),
        ];
    }

    vec![parse_quote!(#abi_ty)]
}

pub(crate) fn synthesize_dispatch_handle_ids(
    self_id: Option<&syn::Type>,
    impl_generics: &syn::Generics,
    sig: &mut syn::Signature,
) {
    let mut synthesized = Punctuated::<syn::FnArg, syn::Token![,]>::new();

    let erased_params = impl_generics
        .type_params()
        .filter(|p| p.attrs.iter().any(is_type_erased))
        .map(|p| &p.ident)
        .collect::<BTreeSet<_>>();

    let explicit_ids = sig
        .inputs
        .iter()
        .filter_map(|input| {
            let syn::FnArg::Typed(syn::PatType { ty, .. }) = input else {
                return None;
            };

            handle_id(ty)
        })
        .collect::<BTreeSet<_>>();

    if erased_params.is_empty() && self_id.is_some() && !explicit_ids.contains(&HandleId::DynSelf) {
        synthesized.push(parse_quote!(__co3_self_id: <dyn Self>::ID));
    }

    for ident in erased_params {
        if explicit_ids.contains(&HandleId::DynType(ident)) {
            continue;
        }

        let pat = format_ident!("{ident}_id");
        synthesized.push(parse_quote!(#pat: <dyn #ident>::ID));
    }

    synthesized.extend(core::mem::take(&mut sig.inputs));
    sig.inputs = synthesized;
}

fn gen_dispatch_arms(
    generics: &syn::Generics,
    trait_: Option<&syn::Path>,
    self_ty: &syn::Type,
    sig: &syn::Signature,
    is_drop_impl: bool,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) -> impl Iterator<Item = TokenStream> {
    let derase_handle_stmts = gen_handle_retype_stmts(RetypeDirection::Derase, generics, sig);

    let dispatch_id_params = sig
        .inputs
        .iter()
        .filter_map(|input| {
            let syn::FnArg::Typed(syn::PatType { ty, .. }) = input else {
                return None;
            };

            handle_id(ty)
        })
        .collect::<Vec<_>>();

    args.iter().map(move |entry| {
        let mut monomorphizer = DispatchMonomorphizer::new(generics, entry);

        let mut arm_sig = sig.clone();
        let fn_name = &arm_sig.ident;
        let callee = if is_drop_impl {
            quote!((|__co3_self: &mut #self_ty| unsafe {
                core::ptr::drop_in_place(__co3_self as *mut _) }
            ))
        } else if let Some(trait_) = trait_ {
            quote!(<#self_ty as #trait_>::#fn_name)
        } else {
            quote!(<#self_ty>::#fn_name)
        };

        let mut patterns = dispatch_id_params
            .iter()
            .map(|handle_id| {
                let handle_ty = match handle_id {
                    HandleId::DynSelf => quote!(#self_ty),
                    HandleId::DynType(ident) => quote!(#ident),
                };

                parse_quote! { <#handle_ty as co3::handle::Handle>::ID }
            })
            .collect::<Vec<syn::Expr>>();

        arm_sig.inputs = arm_sig
            .inputs
            .into_iter()
            .filter(|input| !is_handle_id_arg(input))
            .collect();

        let mut check_sig = arm_sig.clone();
        monomorphizer.visit_signature_mut(&mut check_sig);

        let mut check_callee: syn::Expr = if is_drop_impl {
            parse_quote!((|__co3_self: &mut #self_ty| unsafe {
                core::ptr::drop_in_place(__co3_self as *mut _) }
            ))
        } else if let Some(trait_) = trait_ {
            parse_quote!(<#self_ty as #trait_>::#fn_name)
        } else {
            parse_quote!(<#self_ty>::#fn_name)
        };
        monomorphizer.visit_expr_mut(&mut check_callee);
        let signature_check = gen_fn_signature_check(check_sig, check_callee);
        let arm_body = gen_definition_body(arm_sig, callee);

        let mut arm_body: syn::Block = parse_quote! {{
            (|| -> Result<(), co3::FfiReturn> {
                #(#derase_handle_stmts)*
                #signature_check
                #arm_body
            })()
        }};

        patterns
            .iter_mut()
            .for_each(|pat| monomorphizer.visit_expr_mut(pat));

        monomorphizer.visit_block_mut(&mut arm_body);
        quote! { (#(#patterns,)*) => { #arm_body }}
    })
}

pub(crate) fn erase_handle_types(
    generics: &syn::Generics,
    self_id: Option<&syn::Type>,
    sig: &mut syn::Signature,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) {
    let mut erased_params = ErasedParamReplacer::new(generics);

    let handle_ids = sig
        .inputs
        .iter()
        .enumerate()
        .filter_map(|(idx, input)| {
            let syn::FnArg::Typed(syn::PatType { ty, .. }) = input else {
                return None;
            };

            let handle_ty = match handle_id(ty)? {
                HandleId::DynSelf => self_id.cloned()?,
                HandleId::DynType(ident) => generics
                    .type_params()
                    .find(|p| p.ident == *ident)
                    .and_then(erased_id_repr)?,
            };

            Some((idx, handle_ty))
        })
        .collect::<Vec<_>>();

    for input in &mut sig.inputs {
        match input {
            syn::FnArg::Receiver(rec) => {
                *rec.ty = erased_params.replace((*rec.ty).clone());
            }
            syn::FnArg::Typed(syn::PatType { ty, .. }) => {
                **ty = erased_params.replace((**ty).clone());
            }
        }
    }

    if let syn::ReturnType::Type(_, ty) = &mut sig.output {
        **ty = erased_params.replace((**ty).clone());
    }

    for (idx, lowered_ty) in handle_ids {
        let syn::FnArg::Typed(syn::PatType { ty, .. }) = &mut sig.inputs[idx] else {
            continue;
        };

        **ty = lowered_ty;
    }

    if let Some(entry) = args.first() {
        DispatchMonomorphizer::new(generics, entry).visit_signature_mut(sig);
    }
}

pub(crate) fn gen_handle_erase_stmts(
    self_ty: &syn::Type,
    generics: &syn::Generics,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    let mut sig = sig.clone();

    // TODO: I don't like to clone sig and normalize
    normalize_fn_signature(&mut sig, Some(self_ty));
    gen_handle_retype_stmts(RetypeDirection::Erase, generics, &sig)
}

fn gen_retype(arg_name: &TokenStream, source_ty: &syn::Type, target_ty: &syn::Type) -> TokenStream {
    // NOTE: This transmute is safe because it only works when it's just a pointer cast. When
    // `c_void` is not behind a pointer, it doesn't compile because of missing encode/decode impl
    quote! { unsafe { core::mem::transmute_copy::<#source_ty, #target_ty>(&#arg_name) } }
}

fn gen_handle_retype_stmts(
    direction: RetypeDirection,
    generics: &syn::Generics,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    let handles = sig.inputs.iter().filter(|input| !is_handle_id_arg(input));
    let mut erased_params = ErasedParamReplacer::new(generics);

    let mut stmts = vec![];
    for input in handles {
        let (attrs, arg_name, ty) = match input {
            syn::FnArg::Receiver(receiver) => (&receiver.attrs, quote!(__co3_self), &*receiver.ty),
            syn::FnArg::Typed(syn::PatType { attrs, pat, ty, .. }) => (attrs, quote!(#pat), &**ty),
        };

        let c_ty = item_fn_input_arg_type(attrs, ty);
        let c_ty: syn::Type = parse_quote! { #c_ty };
        let erased_ty = erased_params.replace(c_ty.clone());

        let retype = match direction {
            RetypeDirection::Erase => gen_retype(&arg_name, &c_ty, &erased_ty),
            RetypeDirection::Derase => gen_retype(&arg_name, &erased_ty, &c_ty),
        };

        stmts.push(quote! { let #arg_name = #retype; });
    }

    let syn::ReturnType::Type(_, output_ty) = &sig.output else {
        return stmts;
    };

    let output_ty = unwrap_result_type(output_ty)
        .map(|(ok, _)| ok)
        .unwrap_or(output_ty);

    let out_name = quote!(__co3_out_ptr);
    let out_ptr_ty = output_abi_ty(output_ty);
    let erased_out_ptr_ty = erased_params.replace(out_ptr_ty.clone());
    let out_ptr_arg_ty: syn::Type = parse_quote! { *mut #out_ptr_ty };
    let erased_out_ptr_arg_ty: syn::Type = parse_quote! { *mut #erased_out_ptr_ty };

    let retype_out_ptr = match direction {
        RetypeDirection::Erase => gen_retype(&out_name, &out_ptr_arg_ty, &erased_out_ptr_arg_ty),
        RetypeDirection::Derase => gen_retype(&out_name, &erased_out_ptr_arg_ty, &out_ptr_arg_ty),
    };

    stmts.push(quote! { let __co3_out_ptr = #retype_out_ptr; });
    stmts
}

pub(crate) fn parse_handle_id_attr(attrs: &mut Vec<syn::Attribute>) -> Result<Option<syn::Type>> {
    let mut kept = Vec::with_capacity(attrs.len());

    let mut id_ty = None;
    for attr in attrs.drain(..) {
        if !attr.path().is_ident("id") {
            kept.push(attr);
            continue;
        }

        let syn::Meta::List(list) = &attr.meta else {
            return Err(syn::Error::new_spanned(attr, "expected `#[id(repr)]`"));
        };

        let ty = list
            .parse_args::<syn::Type>()
            .map_err(|_| syn::Error::new_spanned(&attr, "expected `#[id(repr)]`"))?;

        if id_ty.replace(ty).is_some() {
            return Err(syn::Error::new_spanned(attr, "duplicate `#[id(...)]`"));
        }
    }

    *attrs = kept;
    Ok(id_ty)
}

pub(crate) fn handle_id(ty: &syn::Type) -> Option<HandleId<'_>> {
    let syn::Type::Path(syn::TypePath {
        qself:
            Some(syn::QSelf {
                ty,
                position: 0,
                as_token: None,
                ..
            }),
        path,
    }) = ty
    else {
        return None;
    };
    if path.segments.len() != 1 {
        return None;
    }

    let first_seg = path.segments.first()?;
    if first_seg.ident != "ID" || !first_seg.arguments.is_none() {
        return None;
    }

    let syn::Type::TraitObject(arg_ty) = ty.as_ref() else {
        return None;
    };
    if arg_ty.bounds.len() != 1 {
        return None;
    }

    let syn::TypeParamBound::Trait(arg_ty) = arg_ty.bounds.first()? else {
        return None;
    };
    if arg_ty.modifier != syn::TraitBoundModifier::None || arg_ty.lifetimes.is_some() {
        return None;
    }
    if arg_ty.path.segments.len() > 1 {
        return None;
    }
    if let Some(arg_path) = arg_ty.path.get_ident()
        && arg_path == "Self"
    {
        return Some(HandleId::DynSelf);
    }

    Some(HandleId::DynType(&arg_ty.path.segments.first()?.ident))
}

pub(crate) fn is_handle_id_arg(input: &syn::FnArg) -> bool {
    let syn::FnArg::Typed(arg) = input else {
        return false;
    };

    handle_id(&arg.ty).is_some()
}

fn monomorphize_predicates(
    generics: &mut syn::Generics,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) {
    let params = generics
        .params
        .iter()
        .filter_map(|param| {
            if let syn::GenericParam::Type(param) = param {
                return Some(param.ident.clone());
            }

            None
        })
        .collect::<Vec<_>>();

    let mut monomorphized_predicates = Punctuated::new();
    let param_detector = ParamUseDetector::new(&params);

    let old_predicates = generics
        .where_clause
        .as_mut()
        .map(|w| core::mem::take(&mut w.predicates))
        .unwrap_or_default();

    for generic_predicate in old_predicates {
        if !param_detector.predicate_mentions_param(&generic_predicate) {
            monomorphized_predicates.push(generic_predicate);
            continue;
        }

        for entry in args {
            let mut concrete_predicate = generic_predicate.clone();

            let entry = inject_predicate_unnamed_lifetimes(
                &mut generics.params,
                &mut concrete_predicate,
                entry.clone(),
            );

            let mut monomorphizer = DispatchMonomorphizer::new(generics, &entry);
            monomorphizer.visit_where_predicate_mut(&mut concrete_predicate);
            monomorphized_predicates.push(concrete_predicate);
        }
    }

    generics.make_where_clause().predicates = monomorphized_predicates;
}

struct NamedLifetime {
    lifetime: syn::Lifetime,
    universal: bool,
}

struct NamedDispatchEntry {
    entry: syn::AngleBracketedGenericArguments,
    concrete_lifetimes: Vec<syn::Lifetime>,
    universal_lifetimes: Vec<syn::Lifetime>,
}

struct EntryLifetimeNamer {
    prefix: String,
    span: proc_macro2::Span,
    next_lifetime: usize,
    universal_depth: usize,
    lifetimes: Vec<NamedLifetime>,
}

impl EntryLifetimeNamer {
    fn next_lifetime(&mut self) -> syn::Lifetime {
        let lifetime = syn::Lifetime::new(
            &format!("'{}_{}", self.prefix, self.next_lifetime),
            self.span,
        );
        self.next_lifetime += 1;
        lifetime
    }

    fn bind_lifetime(&mut self) -> syn::Lifetime {
        let lifetime = self.next_lifetime();
        self.lifetimes.push(NamedLifetime {
            lifetime: lifetime.clone(),
            universal: self.universal_depth > 0,
        });
        lifetime
    }

    fn visit_universal(&mut self, f: impl FnOnce(&mut Self)) {
        self.universal_depth += 1;
        f(self);
        self.universal_depth -= 1;
    }
}

impl VisitMut for EntryLifetimeNamer {
    fn visit_type_path_mut(&mut self, node: &mut syn::TypePath) {
        if let Some(qself) = &mut node.qself {
            self.visit_type_mut(&mut qself.ty);
            self.visit_universal(|this| {
                for segment in &mut node.path.segments {
                    this.visit_path_arguments_mut(&mut segment.arguments);
                }
            });
            return;
        }

        syn::visit_mut::visit_type_path_mut(self, node);
    }

    fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
        if node.lifetime.as_ref().is_none_or(|l| l.ident == "_") {
            node.lifetime = Some(self.bind_lifetime());
        }

        syn::visit_mut::visit_type_reference_mut(self, node);
    }

    fn visit_lifetime_mut(&mut self, node: &mut syn::Lifetime) {
        if node.ident == "_" {
            *node = self.bind_lifetime();
        }
    }

    fn visit_type_bare_fn_mut(&mut self, _: &mut syn::TypeBareFn) {}
}

fn name_unnamed_lifetimes(
    generics: &Punctuated<GenericParam, syn::Token![,]>,
    mut entry: syn::AngleBracketedGenericArguments,
    mut include_param: impl FnMut(&syn::TypeParam) -> bool,
) -> NamedDispatchEntry {
    let mut concrete_lifetimes = Vec::new();
    let mut universal_lifetimes = Vec::new();

    for (param_idx, (param, arg)) in generics
        .iter()
        .filter(|param| !matches!(param, syn::GenericParam::Lifetime(_)))
        .zip(&mut entry.args)
        .enumerate()
    {
        let syn::GenericParam::Type(param) = param else {
            continue;
        };

        if !include_param(param) {
            continue;
        }

        let mut namer = EntryLifetimeNamer {
            prefix: format!("__co3_dispatch_{param_idx}"),
            span: param.ident.span(),
            next_lifetime: 0,
            universal_depth: 0,
            lifetimes: Vec::new(),
        };
        namer.visit_generic_argument_mut(arg);

        for named in namer.lifetimes {
            if named.universal {
                universal_lifetimes.push(named.lifetime);
            } else {
                concrete_lifetimes.push(named.lifetime);
            }
        }
    }

    NamedDispatchEntry {
        entry,
        concrete_lifetimes,
        universal_lifetimes,
    }
}

fn push_lifetime_param(
    generics: &mut Punctuated<GenericParam, syn::Token![,]>,
    lifetime: &syn::Lifetime,
) {
    if generics.iter().any(
        |param| matches!(param, syn::GenericParam::Lifetime(param) if param.lifetime == *lifetime),
    ) {
        return;
    }

    generics.push(parse_quote!(#lifetime));
}

fn push_lifetime_params(
    generics: &mut Punctuated<GenericParam, syn::Token![,]>,
    lifetimes: &[syn::Lifetime],
) {
    for lifetime in lifetimes {
        push_lifetime_param(generics, lifetime);
    }
}

pub(crate) fn inject_unnamed_lifetimes(
    generics: &mut Punctuated<GenericParam, syn::Token![,]>,
    entry: syn::AngleBracketedGenericArguments,
) -> syn::AngleBracketedGenericArguments {
    let named = name_unnamed_lifetimes(generics, entry, |_| true);
    push_lifetime_params(generics, &named.concrete_lifetimes);
    named.entry
}

fn inject_predicate_unnamed_lifetimes(
    generics: &mut Punctuated<GenericParam, syn::Token![,]>,
    predicate: &mut syn::WherePredicate,
    entry: syn::AngleBracketedGenericArguments,
) -> syn::AngleBracketedGenericArguments {
    struct PredicateLifetimeNamer {
        next_lifetime: usize,
        lifetimes: Vec<syn::Lifetime>,
    }

    impl PredicateLifetimeNamer {
        fn bind_lifetime(&mut self, span: proc_macro2::Span) -> syn::Lifetime {
            let lifetime = syn::Lifetime::new(
                &format!("'__co3_dispatch_predicate_{}", self.next_lifetime),
                span,
            );
            self.next_lifetime += 1;
            self.lifetimes.push(lifetime.clone());
            lifetime
        }
    }

    impl VisitMut for PredicateLifetimeNamer {
        fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
            if node.lifetime.as_ref().is_none_or(|l| l.ident == "_") {
                node.lifetime = Some(self.bind_lifetime(node.span()));
            }

            syn::visit_mut::visit_type_reference_mut(self, node);
        }

        fn visit_lifetime_mut(&mut self, node: &mut syn::Lifetime) {
            if node.ident == "_" {
                *node = self.bind_lifetime(node.span());
            }
        }

        fn visit_type_bare_fn_mut(&mut self, _: &mut syn::TypeBareFn) {}
    }

    fn push_universal_lifetimes(
        predicate: &mut syn::WherePredicate,
        lifetimes: impl IntoIterator<Item = syn::Lifetime>,
    ) {
        let syn::WherePredicate::Type(predicate) = predicate else {
            return;
        };

        let bound_lifetimes = &mut predicate
            .lifetimes
            .get_or_insert_with(|| parse_quote!(for<>))
            .lifetimes;

        for lifetime in lifetimes {
            if bound_lifetimes.iter().any(|param| {
                matches!(param, syn::GenericParam::Lifetime(param) if param.lifetime == lifetime)
            }) {
                continue;
            }

            bound_lifetimes.push(syn::GenericParam::Lifetime(parse_quote!(#lifetime)));
        }
    }

    let mut named = name_unnamed_lifetimes(generics, entry, |param| {
        ParamUseDetector::new([&param.ident]).predicate_mentions_param(predicate)
    });
    push_lifetime_params(generics, &named.concrete_lifetimes);

    let mut predicate_lifetime_namer = PredicateLifetimeNamer {
        next_lifetime: 0,
        lifetimes: Vec::new(),
    };
    predicate_lifetime_namer.visit_where_predicate_mut(predicate);

    named
        .universal_lifetimes
        .extend(predicate_lifetime_namer.lifetimes);
    push_universal_lifetimes(predicate, named.universal_lifetimes);

    named.entry
}
