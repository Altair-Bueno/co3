use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::{
    GenericParam, ReturnType, parse_quote, punctuated::Punctuated, spanned::Spanned, visit::Visit,
    visit_mut::VisitMut,
};

use crate::{
    Co3Fn, Co3Impl, DispatchGroups,
    ffi_fn::{
        self, emit_extern_definition, gen_definition_body, gen_failure_panic,
        gen_fn_signature_check, gen_input_decode_stmts, gen_store_sync_stmts, gen_sync_check,
        gen_sync_error, gen_unknown_handle_error, is_spread_arg, item_fn_input_arg_type,
        item_fn_output_type, merge_generics, normalize_fn_signature, strip_dispatch_params,
    },
    parse::FailureMode,
    utils::{
        DispatchMonomorphizer, ParamUseDetector, erased_abi_repr, erased_id_repr, is_drop_impl,
        is_type_erased,
    },
};

#[derive(Clone, Copy)]
pub(crate) enum DispatchReceiver<'a> {
    None,
    Impl {
        ty: &'a syn::Type,
        id: Option<&'a syn::Type>,
    },
    DynSelf {
        ty: &'a syn::Type,
        id: Option<&'a syn::Type>,
    },
}

impl<'a> DispatchReceiver<'a> {
    pub(crate) fn for_impl(
        original_ty: &'a syn::Type,
        ty: &'a syn::Type,
        id: Option<&'a syn::Type>,
    ) -> Self {
        if crate::trait_object_single_trait_bound(original_ty).is_some() {
            Self::DynSelf { ty, id }
        } else {
            Self::Impl { ty, id }
        }
    }

    fn ty(self) -> Option<&'a syn::Type> {
        match self {
            Self::None => None,
            Self::Impl { ty, .. } | Self::DynSelf { ty, .. } => Some(ty),
        }
    }

    fn id(self) -> Option<&'a syn::Type> {
        match self {
            Self::None => None,
            Self::Impl { id, .. } | Self::DynSelf { id, .. } => id,
        }
    }

    fn is_dyn_self(self) -> bool {
        matches!(self, Self::DynSelf { .. })
    }
}

pub(crate) fn gen_dispatch_fn_export(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    Co3Fn {
        item,
        dispatch_args,
    }: Co3Fn,
) -> TokenStream {
    let handle_id_checks = gen_handle_id_uniqueness_checks(&item.sig.generics, &dispatch_args);

    let fn_name = &item.sig.ident;
    let generics = item.sig.generics.clone();
    let callee = parse_quote!(self::#fn_name);

    let definition = gen_dispatch_definition(
        abi,
        failure_mode,
        &generics,
        DispatchReceiver::None,
        &dispatch_args,
        item.sig,
        &item.attrs,
        &callee,
    );

    quote! {
        const _: () = {
            #handle_id_checks
            #definition
        };
    }
}

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
    attrs.iter().find(|&attr| attr.path().is_ident("erased"))
}

pub(crate) fn gen_dispatch_export(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    Co3Impl {
        item: mut impl_,
        dispatch_args,
        ..
    }: Co3Impl,
    self_id: Option<&syn::Type>,
) -> TokenStream {
    let handle_id_checks = gen_handle_id_uniqueness_checks(&impl_.generics, &dispatch_args);

    let receiver = if crate::materialize_dyn_self_dispatch(&mut impl_) {
        DispatchReceiver::DynSelf {
            ty: &impl_.self_ty,
            id: self_id,
        }
    } else {
        DispatchReceiver::Impl {
            ty: &impl_.self_ty,
            id: self_id,
        }
    };

    let impl_attrs = &impl_.attrs;
    let self_ty = &impl_.self_ty;

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

        let callee: syn::Expr = if drop_impl {
            parse_quote!((|__co3_self: &mut #self_ty| unsafe {
                core::ptr::drop_in_place(__co3_self as *mut _) }
            ))
        } else if let Some(trait_) = impl_.trait_.as_ref().map(|(_, path, _)| path) {
            let fn_name = &item.sig.ident;
            parse_quote!(<#self_ty as #trait_>::#fn_name)
        } else {
            let fn_name = &item.sig.ident;
            parse_quote!(<#self_ty>::#fn_name)
        };

        let definition = gen_dispatch_definition(
            abi,
            failure_mode,
            &impl_.generics,
            receiver,
            &dispatch_args,
            item.sig,
            &item.attrs,
            &callee,
        );

        quote! { #definition }
    });

    quote! {
        #(#impl_attrs)*
        const _: () = {
            #handle_id_checks
            #(#definitions)*
        };
    }
}

#[expect(clippy::too_many_arguments)]
fn gen_dispatch_definition(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    generics: &syn::Generics,
    receiver: DispatchReceiver,
    dispatch_args: &DispatchGroups,
    mut sig: syn::Signature,
    attrs: &[syn::Attribute],
    callee: &syn::Expr,
) -> TokenStream {
    let layout_checks = gen_dispatch_erased_layout_checks(generics, &sig, dispatch_args);

    monomorphize_predicates(&mut sig.generics, dispatch_args);
    strip_dispatch_params(&mut sig.generics);

    let fn_by_val = attrs.iter().any(crate::ffi_fn::is_by_val_attr);
    let selector_inputs = dispatch_selector_inputs(&sig.inputs)
        .filter_map(|(_, pat, handle_id)| {
            let handle_id_ty = resolve_handle_id_type(generics, receiver.id(), handle_id)?;
            Some((pat, handle_id_ty))
        })
        .collect::<Vec<_>>();
    let selector_names = selector_inputs
        .iter()
        .map(|(pat, _)| pat)
        .collect::<Vec<_>>();
    let selector_inputs = selector_inputs
        .iter()
        .map(|(pat, id_ty)| parse_quote!(#pat: #id_ty))
        .collect::<Vec<_>>();

    let dispatch_arms = gen_dispatch_arms(
        generics,
        receiver,
        &sig,
        fn_by_val,
        callee,
        dispatch_args,
        failure_mode,
    );

    let decode_selector_stmts = gen_input_decode_stmts(&selector_inputs, failure_mode);
    let sync_selector_stores = gen_store_sync_stmts(selector_inputs.len());
    let unknown_handle = gen_unknown_handle_error(failure_mode);
    let sync_error = gen_sync_error(failure_mode);
    let selector_sync_check = gen_sync_check(sync_selector_stores, sync_error);

    let fn_body = quote! {{
        #decode_selector_stmts

        let __co3_dispatch_result: core::result::Result<_, _> = match (#(#selector_names,)*) {
            #(#dispatch_arms,)*
            _ => #unknown_handle,
        };

        #selector_sync_check
        __co3_dispatch_result
    }};

    erase_handle_types(generics, receiver, &mut sig);
    let sig = ffi_fn::gen_extern_fn_signature(sig, failure_mode);
    let definition = emit_extern_definition(abi, attrs, failure_mode, sig, fn_body);

    quote! {
        #layout_checks
        #definition
    }
}

pub(crate) fn gen_handle_id_uniqueness_checks(
    generics: &syn::Generics,
    args: &DispatchGroups,
) -> TokenStream {
    fn gen_check(param: &syn::TypeParam, args: &DispatchGroups) -> Option<TokenStream> {
        let repr = erased_id_repr(param)?;

        let (params, entries) = args
            .groups()
            .find(|(params, _)| params.contains(&param.ident))?;

        let enum_ident = format_ident!("DuplicatedHandleIdFor{}", param.ident);
        let param_index = params.iter().position(|ident| *ident == param.ident)?;

        let variants = entries
            .iter()
            .enumerate()
            .filter_map(|(entry_index, entry)| {
                let mut ty = entry.args.get(param_index)?.clone();
                StaticLifetimeNormalizer.visit_generic_argument_mut(&mut ty);

                let variant = format_ident!("HandleId{entry_index}");
                Some(quote! { #variant = <#ty as co3::handle::Handle>::ID })
            })
            .collect::<Vec<_>>();

        if variants.is_empty() {
            return None;
        }

        Some(quote! {
            const _: () = {
                #[repr(#repr)]
                enum #enum_ident {
                    #(#variants,)*
                }
            };
        })
    }

    let checks = generics
        .type_params()
        .filter_map(|param| gen_check(param, args));

    quote! { #(#checks)* }
}

pub(crate) fn gen_dispatch_erased_layout_checks(
    generics: &syn::Generics,
    sig: &syn::Signature,
    args: &DispatchGroups,
) -> TokenStream {
    let layout_param_detector = ParamUseDetector::new(
        generics
            .type_params()
            .filter(|p| p.attrs.iter().any(is_type_erased) && p.default.is_some())
            .map(|p| &p.ident),
    );
    let mut checks = Vec::new();
    args.for_each_combination(|selections| {
        for input in &sig.inputs {
            let (attrs, ty) = match input {
                syn::FnArg::Receiver(receiver) => (&receiver.attrs[..], &*receiver.ty),
                syn::FnArg::Typed(syn::PatType { attrs, ty, .. }) => (&attrs[..], &**ty),
            };

            if handle_id(ty).is_some() {
                continue;
            }

            for layout_ty in dispatch_layout_check_tys(ty, &layout_param_detector) {
                let span = layout_ty.span();
                let mut concrete_ty = layout_ty.clone();
                DispatchMonomorphizer::for_dispatch_group(generics, selections)
                    .visit_type_mut(&mut concrete_ty);
                let concrete_tys = input_abi_tys(attrs, &concrete_ty);
                let erased_ty = ErasedParamReplacer::new(generics).replace(layout_ty);
                let erased_tys = input_abi_tys(attrs, &erased_ty);

                checks.extend(dispatch_layout_checks(
                    concrete_tys
                        .into_iter()
                        .zip(erased_tys)
                        .map(|pair| (pair, span)),
                ));
            }
        }

        let ReturnType::Type(_, output_ty) = &sig.output else {
            return;
        };

        for layout_ty in dispatch_layout_check_tys(output_ty, &layout_param_detector) {
            let span = layout_ty.span();
            let mut concrete_ty = layout_ty.clone();
            DispatchMonomorphizer::for_dispatch_group(generics, selections)
                .visit_type_mut(&mut concrete_ty);
            let concrete_ty = item_fn_output_type(&concrete_ty);
            let erased_ty =
                ErasedParamReplacer::new(generics).replace(item_fn_output_type(&layout_ty));

            checks.extend(dispatch_layout_checks([((concrete_ty, erased_ty), span)]));
        }
    });

    quote! { #(#checks)* }
}

/// Returns the root type and each pointee reached through a reference or raw pointer.
fn dispatch_layout_check_tys(
    ty: &syn::Type,
    param_detector: &ParamUseDetector<'_>,
) -> Vec<syn::Type> {
    struct PointeeCollector {
        tys: Vec<syn::Type>,
    }

    impl PointeeCollector {
        fn visit_pointee(&mut self, pointee: &syn::Type) {
            self.tys.push(pointee.clone());
            self.visit_type(pointee);
        }
    }

    impl Visit<'_> for PointeeCollector {
        fn visit_type_reference(&mut self, node: &syn::TypeReference) {
            self.visit_pointee(&node.elem);
        }

        fn visit_type_ptr(&mut self, node: &syn::TypePtr) {
            self.visit_pointee(&node.elem);
        }

        fn visit_type_bare_fn(&mut self, _: &syn::TypeBareFn) {
            // Function pointer arguments do not describe reinterpreted pointee layouts.
        }
    }

    let mut collector = PointeeCollector {
        tys: vec![ty.clone()],
    };
    collector.visit_type(ty);
    collector
        .tys
        .into_iter()
        .filter(|ty| param_detector.type_mentions_param(ty))
        .collect()
}

fn dispatch_layout_checks(
    pairs: impl IntoIterator<Item = ((syn::Type, syn::Type), Span)>,
) -> Vec<TokenStream> {
    pairs
        .into_iter()
        .map(|((mut concrete_ty, mut erased_ty), span)| {
            StaticLifetimeNormalizer.visit_type_mut(&mut concrete_ty);
            StaticLifetimeNormalizer.visit_type_mut(&mut erased_ty);

            quote_spanned! {span=>
                const {
                    assert!(
                        core::mem::size_of::<#concrete_ty>()
                            == core::mem::size_of::<#erased_ty>(),
                        "tagged-dispatch argument size mismatch",
                    );
                    assert!(
                        core::mem::align_of::<#concrete_ty>()
                            == core::mem::align_of::<#erased_ty>(),
                        "tagged-dispatch argument alignment mismatch",
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

fn input_abi_tys(attrs: &[syn::Attribute], ty: &syn::Type) -> Vec<syn::Type> {
    let abi_ty = item_fn_input_arg_type(attrs, ty);

    if is_spread_arg(attrs) {
        return vec![
            parse_quote!(<#abi_ty as co3::slice::Spread2>::Part1),
            parse_quote!(<#abi_ty as co3::slice::Spread2>::Part2),
        ];
    }

    vec![parse_quote!(#abi_ty)]
}

pub(crate) fn synthesize_dispatch_handle_ids(
    self_id: Option<&syn::Type>,
    impl_generics: &syn::Generics,
    inputs: &mut Punctuated<syn::FnArg, syn::Token![,]>,
) {
    let mut synthesized = Punctuated::<syn::FnArg, syn::Token![,]>::new();

    let erased_params = impl_generics
        .type_params()
        .filter(|p| p.attrs.iter().any(is_type_erased))
        .map(|p| &p.ident)
        .collect::<BTreeSet<_>>();

    let explicit_ids = dispatch_selector_inputs(inputs)
        .map(|(_, _, handle_id)| handle_id)
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

    synthesized.extend(core::mem::take(inputs));
    *inputs = synthesized.into_iter().collect();

    mark_dispatch_handle_ids_by_value(inputs);
}

fn mark_dispatch_handle_ids_by_value(inputs: &mut Punctuated<syn::FnArg, syn::Token![,]>) {
    for input in inputs {
        let syn::FnArg::Typed(input) = input else {
            continue;
        };

        if handle_id(&input.ty).is_some() && !input.attrs.iter().any(ffi_fn::is_by_val_attr) {
            input.attrs.push(parse_quote!(#[by_val]));
        }
    }
}

fn gen_dispatch_arms(
    generics: &syn::Generics,
    receiver: DispatchReceiver,
    sig: &syn::Signature,
    fn_by_val: bool,
    callee: &syn::Expr,
    args: &DispatchGroups,
    failure_mode: FailureMode,
) -> Vec<TokenStream> {
    let derase_handle_stmts =
        gen_handle_retype_stmts(RetypeDirection::Derase, generics, receiver, sig);

    let dispatch_selectors = dispatch_selector_inputs(&sig.inputs)
        .map(|(_, _, handle_id)| handle_id)
        .collect::<Vec<_>>();

    let mut arms = Vec::new();
    args.for_each_combination(|selections| {
        let mut monomorphizer = DispatchMonomorphizer::for_dispatch_group(generics, selections);
        let mut arm_sig = sig.clone();
        let mut patterns = dispatch_selectors
            .iter()
            .map(|handle_id| {
                let handle_ty = match handle_id {
                    HandleId::DynSelf => {
                        let ty = receiver
                            .ty()
                            .expect("`dyn Self` selector requires an impl self type");
                        quote!(#ty)
                    }
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

        let retype_sig = arm_sig.clone();
        monomorphizer.visit_signature_mut(&mut arm_sig);
        let mut check_callee = callee.clone();

        monomorphizer.visit_expr_mut(&mut check_callee);
        let signature_check = gen_fn_signature_check(arm_sig.clone(), check_callee);
        let arm_body = gen_definition_body(arm_sig, quote!(#callee), fn_by_val, failure_mode);
        let mut arm_body: syn::Block = if let ReturnType::Type(_, output_ty) = &retype_sig.output {
            let mut concrete_ty = item_fn_output_type(output_ty);
            monomorphizer.visit_type_mut(&mut concrete_ty);

            let erased_ty =
                ErasedParamReplacer::new(generics).replace(item_fn_output_type(output_ty));
            let erased_out = gen_retype(&quote!(__co3_arm_out), &concrete_ty, &erased_ty);

            let erased_err = match failure_mode {
                FailureMode::Panic => {
                    let failure_panic = gen_failure_panic(quote!(__co3_arm_err));
                    quote! { Err(__co3_arm_err) => #failure_panic, }
                }
                FailureMode::Error => {
                    let erased_err = gen_retype(&quote!(__co3_arm_err), &concrete_ty, &erased_ty);

                    quote! {
                        Err(__co3_arm_err) => {
                            let __co3_arm_err = co3::encode(__co3_arm_err);
                            Ok(#erased_err)
                        },
                    }
                }
            };

            parse_quote! {{
                match (|| -> Result<_, _> {
                    #(#derase_handle_stmts)*
                    #signature_check
                    #arm_body
                })() {
                    Ok(__co3_arm_out) => Ok::<_, ()>(#erased_out),
                    #erased_err
                }
            }}
        } else {
            parse_quote! {{
                (|| -> Result<_, _> {
                    #(#derase_handle_stmts)*
                    #signature_check
                    #arm_body
                })()
            }}
        };

        patterns
            .iter_mut()
            .for_each(|pat| monomorphizer.visit_expr_mut(pat));

        monomorphizer.visit_block_mut(&mut arm_body);
        arms.push(quote! { (#(#patterns,)*) => { #arm_body }});
    });
    arms
}

pub(crate) fn erase_handle_types(
    generics: &syn::Generics,
    receiver: DispatchReceiver,
    sig: &mut syn::Signature,
) {
    let mut erased_params = ErasedParamReplacer::new(generics);

    let handle_ids = dispatch_selector_inputs(&sig.inputs)
        .filter_map(|(idx, _, handle_id)| {
            Some((
                idx,
                resolve_handle_id_type(generics, receiver.id(), handle_id)?,
            ))
        })
        .collect::<Vec<_>>();

    for input in &mut sig.inputs {
        match input {
            syn::FnArg::Receiver(rec) => {
                *rec.ty = erased_params.replace((*rec.ty).clone());
            }
            syn::FnArg::Typed(syn::PatType { pat, ty, .. }) => {
                **ty = erased_params.replace((**ty).clone());

                if receiver.is_dyn_self() && is_synthetic_receiver(pat) {
                    **ty = erase_dyn_self_type(ty);
                }
            }
        }
    }

    if let ReturnType::Type(_, ty) = &mut sig.output {
        **ty = erased_params.replace((**ty).clone());
    }

    for (idx, lowered_ty) in handle_ids {
        let syn::FnArg::Typed(syn::PatType { ty, .. }) = &mut sig.inputs[idx] else {
            continue;
        };

        **ty = lowered_ty;
    }
}

pub(crate) fn gen_handle_erase_stmts(
    self_ty: &syn::Type,
    generics: &syn::Generics,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    let mut sig = sig.clone();

    normalize_fn_signature(&mut sig, Some(self_ty));
    gen_handle_retype_stmts(
        RetypeDirection::Erase,
        generics,
        DispatchReceiver::Impl {
            ty: self_ty,
            id: None,
        },
        &sig,
    )
}

fn is_synthetic_receiver(pat: &syn::Pat) -> bool {
    matches!(pat, syn::Pat::Ident(ident) if ident.ident == "__co3_self")
}

fn erase_dyn_self_type(ty: &syn::Type) -> syn::Type {
    let mut erased = ty.clone();

    if let syn::Type::Reference(reference) = &mut erased {
        *reference.elem = parse_quote!(core::ffi::c_void);
    } else {
        unreachable!("Opaque type must be behind an indirection")
    }

    erased
}

fn gen_retype(arg_name: &TokenStream, source_ty: &syn::Type, target_ty: &syn::Type) -> TokenStream {
    // TODO: Write a safety comment.
    quote! { unsafe { core::mem::transmute_copy::<#source_ty, #target_ty>(&#arg_name) } }
}

pub(crate) fn gen_return_derase_expr(
    generics: &syn::Generics,
    output_ty: &syn::Type,
    value: TokenStream,
) -> TokenStream {
    let concrete_ty = item_fn_output_type(output_ty);
    let erased_ty = ErasedParamReplacer::new(generics).replace(concrete_ty.clone());

    gen_retype(&value, &erased_ty, &concrete_ty)
}

fn gen_handle_retype_stmts(
    direction: RetypeDirection,
    generics: &syn::Generics,
    receiver: DispatchReceiver,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    let handles = sig.inputs.iter().filter(|input| !is_handle_id_arg(input));
    let mut erased_params = ErasedParamReplacer::new(generics);

    let mut stmts = vec![];
    for input in handles {
        let (attrs, arg_name, ty, is_self) = match input {
            syn::FnArg::Receiver(receiver) => {
                (&receiver.attrs, quote!(__co3_self), &*receiver.ty, true)
            }
            syn::FnArg::Typed(syn::PatType { attrs, pat, ty, .. }) => {
                (attrs, quote!(#pat), &**ty, is_synthetic_receiver(pat))
            }
        };

        let c_ty = item_fn_input_arg_type(attrs, ty);
        let c_ty: syn::Type = parse_quote! { #c_ty };
        let erased_ty = if receiver.is_dyn_self() && is_self {
            let erased_self_ty = erase_dyn_self_type(ty);
            let erased_c_ty = item_fn_input_arg_type(attrs, &erased_self_ty);
            parse_quote!(#erased_c_ty)
        } else {
            erased_params.replace(c_ty.clone())
        };

        let retype = match direction {
            RetypeDirection::Erase => gen_retype(&arg_name, &c_ty, &erased_ty),
            RetypeDirection::Derase => gen_retype(&arg_name, &erased_ty, &c_ty),
        };

        stmts.push(quote! { let #arg_name = #retype; });
    }

    stmts
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

fn dispatch_selector_inputs(
    inputs: &Punctuated<syn::FnArg, syn::Token![,]>,
) -> impl Iterator<Item = (usize, &syn::Pat, HandleId<'_>)> {
    inputs.iter().enumerate().filter_map(|(idx, input)| {
        let syn::FnArg::Typed(syn::PatType { pat, ty, .. }) = input else {
            return None;
        };

        Some((idx, pat.as_ref(), handle_id(ty)?))
    })
}

fn resolve_handle_id_type(
    generics: &syn::Generics,
    self_id: Option<&syn::Type>,
    handle_id: HandleId<'_>,
) -> Option<syn::Type> {
    match handle_id {
        HandleId::DynSelf => self_id.cloned(),
        HandleId::DynType(ident) => generics
            .type_params()
            .find(|param| param.ident == *ident)
            .and_then(erased_id_repr),
    }
}

fn monomorphize_predicates(generics: &mut syn::Generics, args: &DispatchGroups) {
    fn strip_relaxed_sized_bounds(predicate: &mut syn::WherePredicate) -> bool {
        let syn::WherePredicate::Type(predicate) = predicate else {
            return true;
        };

        predicate.bounds = core::mem::take(&mut predicate.bounds)
            .into_iter()
            .filter(|bound| {
                !matches!(
                    bound,
                    syn::TypeParamBound::Trait(bound)
                        if matches!(bound.modifier, syn::TraitBoundModifier::Maybe(_))
                )
            })
            .collect();

        !predicate.bounds.is_empty()
    }

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

        args.for_each_combination(|selections| {
            let mut concrete_predicate = generic_predicate.clone();

            let mut concrete_args: syn::AngleBracketedGenericArguments = syn::parse_quote!(<>);
            concrete_args.args.extend(
                selections
                    .iter()
                    .flat_map(|selection| selection.target.args.iter().cloned()),
            );

            let args = inject_predicate_unnamed_lifetimes(
                &mut generics.params,
                &mut concrete_predicate,
                concrete_args,
            );

            let mut monomorphizer = DispatchMonomorphizer::for_substitutions(
                generics,
                selections
                    .iter()
                    .flat_map(|selection| selection.params.iter())
                    .zip(&args.args),
            );
            monomorphizer.visit_where_predicate_mut(&mut concrete_predicate);
            if strip_relaxed_sized_bounds(&mut concrete_predicate) {
                monomorphized_predicates.push(concrete_predicate);
            }
        });
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
