use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    GenericParam, ReturnType, parse_quote, punctuated::Punctuated, spanned::Spanned,
    visit_mut::VisitMut,
};

use crate::{
    DynImpl,
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
    dispatch: DynImpl,
    self_id: Option<&syn::Type>,
) -> TokenStream {
    let DynImpl { impl_, args } = dispatch;

    let trait_ = impl_.trait_.as_ref().map(|(_, path, _)| path);
    let self_ty = &impl_.self_ty;
    let generics = &impl_.generics;
    let impl_attrs = &impl_.attrs;
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
        strip_dispatch_params(&mut item.sig.generics);

        let fn_by_val = item.attrs.iter().any(crate::ffi_fn::is_by_val_attr);
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

        let dispatch_arms = gen_dispatch_arms(
            generics,
            trait_,
            self_ty,
            &item.sig,
            fn_by_val,
            drop_impl,
            &args,
            failure_mode,
        )
        .collect::<Vec<_>>();

        let decode_id_stmts = gen_input_decode_stmts(&handle_ids, failure_mode);
        let sync_id_stores = gen_store_sync_stmts(handle_ids.len());
        let unknown_handle = gen_unknown_handle_error(failure_mode);
        let sync_error = gen_sync_error(failure_mode);
        let id_sync_check = gen_sync_check(sync_id_stores, sync_error);

        let fn_body = quote! {{
            #decode_id_stmts

            let __co3_dispatch_result: core::result::Result<_, _> = match (#(#id_arg_names,)*) {
                #(#dispatch_arms,)*
                _ => #unknown_handle,
            };

            #id_sync_check
            __co3_dispatch_result
        }};

        erase_handle_types(generics, self_id, &mut item.sig, &args);
        let sig = ffi_fn::gen_extern_fn_signature(item.sig, failure_mode);
        let definition = emit_extern_definition(abi, &item.attrs, failure_mode, sig, fn_body);

        quote! {
            #erased_layout_checks
            #definition
        }
    });

    quote! {
        #(#impl_attrs)*
        const _: () = {
            #dispatch_id_checks
            #(#definitions)*
        };
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

        let ReturnType::Type(_, output_ty) = &sig.output else {
            return checks;
        };

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
    item_fn_output_type(&ty)
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
    item_fn_output_type(&ty)
}

fn input_abi_tys(attrs: &[syn::Attribute], ty: &syn::Type) -> Vec<syn::Type> {
    let abi_ty = item_fn_input_arg_type(attrs, ty);

    if is_spread_arg(attrs) {
        return vec![
            parse_quote!(<#abi_ty as co3::slice::Spread>::Part1),
            parse_quote!(<#abi_ty as co3::slice::Spread>::Part2),
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

#[expect(clippy::too_many_arguments)]
fn gen_dispatch_arms(
    generics: &syn::Generics,
    trait_: Option<&syn::Path>,
    self_ty: &syn::Type,
    sig: &syn::Signature,
    fn_by_val: bool,
    is_drop_impl: bool,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
    failure_mode: FailureMode,
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
        let arm_body = gen_definition_body(arm_sig.clone(), callee, fn_by_val, failure_mode);
        let mut arm_body: syn::Block = if let ReturnType::Type(_, output_ty) = &arm_sig.output {
            let concrete_ty = item_fn_output_type(output_ty);

            let erased_ty = ErasedParamReplacer::new(generics).replace(concrete_ty.clone());
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

    if let ReturnType::Type(_, ty) = &mut sig.output {
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

fn monomorphize_predicates(
    generics: &mut syn::Generics,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) {
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

        for entry in args {
            let mut concrete_predicate = generic_predicate.clone();

            let entry = inject_predicate_unnamed_lifetimes(
                &mut generics.params,
                &mut concrete_predicate,
                entry.clone(),
            );

            let mut monomorphizer = DispatchMonomorphizer::new(generics, &entry);
            monomorphizer.visit_where_predicate_mut(&mut concrete_predicate);
            if strip_relaxed_sized_bounds(&mut concrete_predicate) {
                monomorphized_predicates.push(concrete_predicate);
            }
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
