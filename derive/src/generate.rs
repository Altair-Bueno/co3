use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ImplItem, ImplItemFn, ItemImpl, punctuated::Punctuated, visit_mut::VisitMut};

use crate::{
    Co3Fn, Co3Impl, DispatchGroups, ForeignItem, ForeignItemType, co3_path,
    dispatch::{
        StaticLifetimeNormalizer, erase_dispatch_signature, gen_dispatch_erased_layout_checks,
        gen_dispatch_export, gen_dispatch_fn_export, gen_handle_id_type_checks,
    },
    ffi_fn::{
        self, gen_extern_fn_signature, merge_generics, normalize_fn_signature,
        strip_dispatch_params,
    },
    parse::{FailureMode, MacroFeatures},
    trait_object_single_trait_bound,
    utils::{
        DispatchMonomorphizer, ParamUseDetector, cfg_attrs, erased_id_repr,
        has_non_lifetime_generics, is_type_erased, soft_for_arg, strip_internal_generic_param,
    },
    wrapper::{
        gen_extern_decl, gen_impl_wrapper_body, gen_wrapper_body, gen_wrapper_body_with_callee,
        strip_internal_arg_attrs, wrap_fn_definition, wrap_impl_definition,
    },
};

fn lift_dispatch_method(mut dispatch: Co3Impl) -> Co3Impl {
    let syn::ImplItem::Fn(method) = dispatch.items.first_mut().unwrap() else {
        unreachable!()
    };
    let method_generics = core::mem::take(&mut method.sig.generics);
    dispatch.generics = combine_dispatch_generics(Some(&dispatch.generics), &method_generics);

    dispatch
}

/// Flattens the nested impl and method generic scopes for generated dispatch machinery.
///
/// Rust requires lifetime parameters to precede type and const parameters. Within those
/// categories, keep the outer (impl) scope before the inner (method) scope and preserve each
/// declaration's source order.
fn combine_dispatch_generics(
    impl_generics: Option<&syn::Generics>,
    method_generics: &syn::Generics,
) -> syn::Generics {
    let mut combined = impl_generics.cloned().unwrap_or_default();
    merge_generics(method_generics.clone(), &mut combined);

    let (lifetimes, non_lifetimes): (Vec<_>, Vec<_>) = combined
        .params
        .into_iter()
        .partition(|param| matches!(param, syn::GenericParam::Lifetime(_)));
    combined.params = lifetimes.into_iter().chain(non_lifetimes).collect();
    combined
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
        let method_dispatch_args = impl_.method_dispatch_args.remove(&method.sig.ident);
        if method_dispatch_args.is_none() && !has_payload_dispatch(&method.sig.generics) {
            plain_items.push(syn::ImplItem::Fn(method));
            continue;
        }
        let method_dispatch_args = method_dispatch_args.unwrap_or_default();
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

fn move_method_only_impl_params(impl_: &mut ItemImpl, method: &mut ImplItemFn) {
    let appears_in_impl_identity = |ident: &syn::Ident| {
        let detector = ParamUseDetector::new([ident]);
        detector.type_mentions_param(&impl_.self_ty)
            || impl_
                .trait_
                .as_ref()
                .is_some_and(|(path, _)| detector.path_mentions_param(path))
    };
    let moved_params = impl_
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            syn::GenericParam::Type(param) if !appears_in_impl_identity(&param.ident) => {
                Some(param.ident.clone())
            }
            syn::GenericParam::Const(param) if !appears_in_impl_identity(&param.ident) => {
                Some(param.ident.clone())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if moved_params.is_empty() {
        return;
    }

    impl_.generics.params = core::mem::take(&mut impl_.generics.params)
        .into_iter()
        .filter_map(|param| {
            let move_to_method = match &param {
                syn::GenericParam::Type(param) => moved_params.contains(&param.ident),
                syn::GenericParam::Const(param) => moved_params.contains(&param.ident),
                syn::GenericParam::Lifetime(_) => false,
            };
            if move_to_method {
                method.sig.generics.params.push(param);
                None
            } else {
                Some(param)
            }
        })
        .collect();

    let moved = ParamUseDetector::new(&moved_params);
    let Some(where_clause) = impl_.generics.where_clause.as_mut() else {
        return;
    };
    where_clause.predicates = core::mem::take(&mut where_clause.predicates)
        .into_iter()
        .filter_map(|predicate| {
            if moved.predicate_mentions_param(&predicate) {
                method
                    .sig
                    .generics
                    .make_where_clause()
                    .predicates
                    .push(predicate);
                None
            } else {
                Some(predicate)
            }
        })
        .collect();
    if where_clause.predicates.is_empty() {
        impl_.generics.where_clause = None;
    }
}

fn monomorphize_static_impl_bindings(
    dispatch: Co3Impl,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> Vec<Co3Impl> {
    let generics = dispatch.generics.clone();
    dispatch
        .dispatch_args
        .static_bindings(&generics)
        .into_iter()
        .map(|(static_args, dynamic_args)| {
            let mut binding = dispatch.clone();
            static_args.for_each_combination(|selections| {
                let mut monomorphizer =
                    DispatchMonomorphizer::for_static_dispatch_group(&generics, selections);
                monomorphizer.visit_item_impl_mut(&mut binding.item);
                for item in &mut binding.item.items {
                    let syn::ImplItem::Fn(method) = item else {
                        continue;
                    };
                    monomorphizer.interpolate_symbol_attrs(&mut method.attrs, symbol_fragments);
                }
            });
            binding.item.generics.params = core::mem::take(&mut binding.item.generics.params)
                .into_iter()
                .filter(|param| match param {
                    syn::GenericParam::Lifetime(_) => true,
                    syn::GenericParam::Type(param) => !static_args.contains_param(&param.ident),
                    syn::GenericParam::Const(param) => !static_args.contains_param(&param.ident),
                })
                .collect();
            binding.dispatch_args = dynamic_args;
            binding
        })
        .collect()
}

fn monomorphize_static_fn_bindings(
    dispatch: Co3Fn,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> Vec<(Co3Fn, syn::Expr)> {
    let generics = dispatch.sig.generics.clone();
    let fn_name = &dispatch.sig.ident;
    let generic_args = generics
        .params
        .iter()
        .filter_map(|param| match param {
            syn::GenericParam::Lifetime(_) => None,
            syn::GenericParam::Type(param) => {
                let ident = &param.ident;
                Some(syn::GenericArgument::Type(syn::parse_quote!(#ident)))
            }
            syn::GenericParam::Const(param) => {
                let ident = &param.ident;
                Some(syn::GenericArgument::Const(syn::parse_quote!(#ident)))
            }
        })
        .collect::<Punctuated<_, syn::Token![,]>>();
    let mut callee: syn::Expr = syn::parse_quote!(self::#fn_name);
    if !generic_args.is_empty() {
        let syn::Expr::Path(path) = &mut callee else {
            unreachable!()
        };
        path.path.segments.last_mut().unwrap().arguments =
            syn::PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments {
                colon2_token: Some(Default::default()),
                lt_token: Default::default(),
                args: generic_args,
                gt_token: Default::default(),
            });
    }

    dispatch
        .dispatch_args
        .static_bindings(&generics)
        .into_iter()
        .map(|(static_args, dynamic_args)| {
            let mut binding = dispatch.clone();
            let mut callee = callee.clone();
            static_args.for_each_combination(|selections| {
                let mut monomorphizer =
                    DispatchMonomorphizer::for_static_dispatch_group(&generics, selections);
                monomorphizer.visit_item_fn_mut(&mut binding.item);
                monomorphizer.visit_expr_mut(&mut callee);
                monomorphizer.interpolate_symbol_attrs(&mut binding.attrs, symbol_fragments);
            });
            binding.sig.generics.params = core::mem::take(&mut binding.sig.generics.params)
                .into_iter()
                .filter(|param| match param {
                    syn::GenericParam::Lifetime(_) => true,
                    syn::GenericParam::Type(param) => !static_args.contains_param(&param.ident),
                    syn::GenericParam::Const(param) => !static_args.contains_param(&param.ident),
                })
                .collect();
            binding.dispatch_args = dynamic_args;
            (binding, callee)
        })
        .collect()
}

fn materialize_dyn_self_receiver(impl_: &mut ItemImpl) {
    let Some(bound) = crate::trait_object_single_trait_bound(&impl_.self_ty) else {
        return;
    };
    let path = &bound.path;
    impl_.self_ty = syn::parse_quote!(#path);
}

fn materialize_declared_dyn_self(impl_: &mut ItemImpl, declared_self: bool) -> bool {
    let dyn_self = declared_self && trait_object_single_trait_bound(&impl_.self_ty).is_some();
    if dyn_self {
        materialize_dyn_self_receiver(impl_);
    }
    dyn_self
}

fn gen_export_impl(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    impl_: Co3Impl,
    type_id: Option<&syn::Type>,
    declared_self: bool,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> TokenStream {
    let (plain, methods) = split_dyn_methods(impl_);
    let descriptors = core::iter::once(plain)
        .chain(methods.into_iter().map(lift_dispatch_method))
        .flat_map(|mut descriptor| {
            let dyn_self = materialize_declared_dyn_self(&mut descriptor.item, declared_self);
            monomorphize_static_impl_bindings(descriptor, symbol_fragments)
                .into_iter()
                .map(move |descriptor| (descriptor, dyn_self))
        });
    let definitions = descriptors.map(|(descriptor, dyn_self)| {
        if descriptor.items.is_empty() {
            TokenStream::new()
        } else if !descriptor.dispatch_args.is_empty() || dyn_self {
            let self_id = dyn_self.then_some(type_id).flatten();
            gen_dispatch_export(abi, failure_mode, descriptor, self_id, dyn_self)
        } else {
            ffi_fn::gen_impl_definition(abi, failure_mode, descriptor.item)
        }
    });

    quote!(#(#definitions)*)
}

fn dispatch_type_idents(generics: &syn::Generics) -> Vec<syn::Ident> {
    generics
        .type_params()
        .filter(|param| param.attrs.iter().any(is_type_erased))
        .map(|param| param.ident.clone())
        .collect()
}

fn has_static_dispatch(generics: &syn::Generics, args: &DispatchGroups) -> bool {
    generics.params.iter().any(|param| match param {
        syn::GenericParam::Lifetime(_) => false,
        syn::GenericParam::Type(param) => {
            !param.attrs.iter().any(is_type_erased) && args.contains_param(&param.ident)
        }
        syn::GenericParam::Const(param) => args.contains_param(&param.ident),
    })
}

fn has_payload_dispatch(generics: &syn::Generics) -> bool {
    generics
        .type_params()
        .any(|param| param.attrs.iter().any(is_type_erased) && param.default.is_some())
}

/// Builds the dispatch-set generic arguments in source declaration order.
fn dispatch_trait_args(
    generics: &syn::Generics,
) -> Punctuated<syn::GenericArgument, syn::Token![,]> {
    generics
        .params
        .iter()
        .map(|param| match param {
            syn::GenericParam::Lifetime(param) => {
                syn::GenericArgument::Lifetime(param.lifetime.clone())
            }
            syn::GenericParam::Type(param) => {
                let ident = &param.ident;
                syn::parse_quote!(#ident)
            }
            syn::GenericParam::Const(param) => {
                let ident = &param.ident;
                syn::GenericArgument::Const(syn::parse_quote!(#ident))
            }
        })
        .collect()
}

fn dispatch_trait_generics(generics: &syn::Generics, args: &DispatchGroups) -> syn::Generics {
    let is_maybe_sized = |bound: &syn::TypeParamBound| {
        matches!(bound, syn::TypeParamBound::Trait(bound)
            if bound.maybe.is_some()
                && bound.path.is_ident("Sized"))
    };
    let maybe_sized = generics
        .where_clause
        .iter()
        .flat_map(|clause| &clause.predicates)
        .filter_map(|predicate| match predicate {
            syn::WherePredicate::Type(predicate)
                if predicate.bounds.iter().any(&is_maybe_sized) =>
            {
                let syn::Type::Path(ty) = &predicate.bounded_ty else {
                    return None;
                };
                ty.qself.is_none().then(|| ty.path.get_ident()).flatten()
            }
            _ => None,
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    let dispatched_params = generics
        .params
        .iter()
        .filter_map(|param| match param {
            syn::GenericParam::Type(param) if args.contains_param(&param.ident) => {
                Some(param.ident.clone())
            }
            syn::GenericParam::Const(param) if args.contains_param(&param.ident) => {
                Some(param.ident.clone())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    let mut trait_generics = generics.clone();
    trait_generics.type_params_mut().for_each(|param| {
        let is_maybe_sized =
            param.bounds.iter().any(&is_maybe_sized) || maybe_sized.contains(&param.ident);
        strip_internal_generic_param(param);
        if args.contains_param(&param.ident) {
            param.bounds.clear();
            if is_maybe_sized {
                param.bounds.push(syn::parse_quote!(?Sized));
            }
        }
    });
    if let Some(where_clause) = &mut trait_generics.where_clause {
        let dispatched = ParamUseDetector::new(&dispatched_params);
        where_clause.predicates = core::mem::take(&mut where_clause.predicates)
            .into_iter()
            .filter(|predicate| !dispatched.predicate_mentions_param(predicate))
            .collect();
        if where_clause.predicates.is_empty() {
            trait_generics.where_clause = None;
        }
    }
    trait_generics
}

fn dispatch_membership_bound(
    dispatch_set: &TokenStream,
    trait_args: &Punctuated<syn::GenericArgument, syn::Token![,]>,
) -> syn::WherePredicate {
    syn::parse_quote!((): #dispatch_set<#trait_args>)
}

fn dispatch_set_name() -> syn::Ident {
    format_ident!("DispatchSet")
}

fn dispatch_module_name(self_ty: &syn::Type, method: &syn::Ident) -> syn::Ident {
    let receiver = match self_ty {
        syn::Type::Path(syn::TypePath {
            qself: None, path, ..
        }) => path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_else(|| "dispatch".to_owned()),
        _ => "dispatch".to_owned(),
    };
    format_ident!("{}_{}", receiver.to_lowercase(), method)
}

struct DispatchSetDelegate<'a> {
    abi: &'a syn::Abi,
    block_attrs: &'a [syn::Attribute],
    fn_attrs: &'a [syn::Attribute],
    failure_mode: FailureMode,
    source_sig: &'a syn::Signature,
    wrapper_sig: &'a syn::Signature,
    raw_sig: &'a syn::Signature,
    self_ty: Option<&'a syn::Type>,
    dispatch_generics: &'a syn::Generics,
    declared_self: bool,
    symbol_fragments: &'a std::collections::BTreeMap<String, syn::LitStr>,
}

fn dispatch_delegate_sig(
    wrapper_sig: &syn::Signature,
    self_ty: Option<&syn::Type>,
    delegate_name: &syn::Ident,
) -> syn::Signature {
    let mut sig = wrapper_sig.clone();
    normalize_fn_signature(&mut sig, self_ty);
    sig.ident = delegate_name.clone();
    sig.generics.params.clear();
    sig
}

fn dispatch_source_arg_names(sig: &syn::Signature) -> Vec<TokenStream> {
    sig.inputs
        .iter()
        .filter(|input| !crate::dispatch::is_handle_id_arg(input))
        .map(|input| match input {
            FnArg::Typed(input) => {
                let ident = ffi_fn::item_fn_input_ident(&input.pat);
                quote!(#ident)
            }
            FnArg::Receiver(_) => quote!(__co3_self),
        })
        .collect()
}

fn gen_dispatch_set(
    trait_name: &syn::Ident,
    sealed_module: &syn::Ident,
    generics: &syn::Generics,
    args: &DispatchGroups,
    delegate: Option<&DispatchSetDelegate<'_>>,
) -> TokenStream {
    let trait_generics = dispatch_trait_generics(generics, args);
    let trait_args = dispatch_trait_args(&trait_generics);
    let trait_where_clause = &trait_generics.where_clause;
    let trait_params = &trait_generics.params;
    let trait_decl_generics = (!trait_params.is_empty()).then(|| quote!(<#trait_params>));
    let dispatch_trait_items = delegate
        .map(|delegate| {
            let sig = dispatch_delegate_sig(
                delegate.wrapper_sig,
                delegate.self_ty,
                &delegate.source_sig.ident,
            );
            quote!(#sig;)
        })
        .unwrap_or_default();
    let mut impls = Vec::new();
    args.for_each_combination(|selections| {
        let mut selection_args = dispatch_trait_args(generics);
        let mut monomorphizer = DispatchMonomorphizer::for_dispatch_group(generics, selections);
        selection_args
            .iter_mut()
            .for_each(|arg| monomorphizer.visit_generic_argument_mut(arg));
        if let Some(self_ty) = delegate.and_then(|delegate| delegate.self_ty) {
            selection_args.iter_mut().for_each(|arg| {
                ffi_fn::SelfConcretizer { self_ty }.visit_generic_argument_mut(arg);
                monomorphizer.visit_generic_argument_mut(arg);
            });
        }
        let lifetimes = generics.lifetimes().collect::<Vec<_>>();
        let auxiliary_params = generics
            .params
            .iter()
            .filter_map(|param| match param {
                syn::GenericParam::Type(param) if !args.contains_param(&param.ident) => {
                    let mut param = param.clone();
                    strip_internal_generic_param(&mut param);
                    Some(syn::GenericParam::Type(param))
                }
                syn::GenericParam::Const(param) if !args.contains_param(&param.ident) => {
                    let mut param = param.clone();
                    param.default = None;
                    Some(syn::GenericParam::Const(param))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let impl_generics = (!lifetimes.is_empty() || !auxiliary_params.is_empty())
            .then(|| quote!(<#(#lifetimes,)* #(#auxiliary_params),*>));

        let dispatch_impl_items = delegate
            .map(|delegate| {
                let mut raw_sig = delegate.raw_sig.clone();
                monomorphizer.visit_signature_mut(&mut raw_sig);
                if let Some(self_ty) = delegate.self_ty {
                    ffi_fn::SelfConcretizer { self_ty }.visit_signature_mut(&mut raw_sig);
                    monomorphizer.visit_signature_mut(&mut raw_sig);
                }
                raw_sig.ident = format_ident!("__co3_raw");
                raw_sig.safety = syn::Safety::Default;
                let mut attrs = delegate.fn_attrs.to_vec();
                monomorphizer.interpolate_symbol_attrs(&mut attrs, delegate.symbol_fragments);
                let extern_decl =
                    gen_extern_decl(delegate.abi, delegate.block_attrs, &attrs, quote!(#raw_sig));

                // Generate conversion and erasure while the dispatch
                // parameters are still visible, then monomorphize the whole
                // method. This preserves the casts that dynamic dispatch needs
                // while making static parameters concrete inside each impl.
                let sig = dispatch_delegate_sig(
                    delegate.wrapper_sig,
                    delegate.self_ty,
                    &delegate.source_sig.ident,
                );
                let id_assignments = dispatch_id_assignments(delegate.source_sig, delegate.self_ty);
                let dummy_self_ty = syn::parse_quote!(());
                let wrapper_body = gen_wrapper_body_with_callee::<true>(
                    delegate.failure_mode,
                    delegate.fn_attrs.iter().any(ffi_fn::is_by_val_attr),
                    Some(delegate.self_ty.unwrap_or(&dummy_self_ty)),
                    Some(delegate.dispatch_generics),
                    delegate.declared_self,
                    delegate.source_sig,
                    quote!(__co3_raw),
                );
                let co3 = co3_path();
                let method_tokens = quote! {
                    #sig {
                        use #co3 as co3;
                        #extern_decl
                        #(#id_assignments)*
                        #wrapper_body
                    }
                };
                let mut method: syn::ImplItemFn = syn::parse2(method_tokens.clone())
                    .unwrap_or_else(|error| {
                        panic!("invalid dispatch delegate `{method_tokens}`: {error}")
                    });
                monomorphizer.visit_impl_item_fn_mut(&mut method);
                if let Some(self_ty) = delegate.self_ty {
                    ffi_fn::SelfConcretizer { self_ty }.visit_impl_item_fn_mut(&mut method);
                    monomorphizer.visit_impl_item_fn_mut(&mut method);
                }
                quote!(#method)
            })
            .unwrap_or_default();

        impls.push(quote! {
            impl #impl_generics #sealed_module::Sealed<#selection_args> for () #trait_where_clause {}
            impl #impl_generics #trait_name<#selection_args> for () #trait_where_clause {
                #dispatch_impl_items
            }
        });
    });

    quote! {
        mod #sealed_module {
            use super::*;

            pub trait Sealed #trait_decl_generics #trait_where_clause {}
        }
        pub(super) trait #trait_name #trait_decl_generics:
            #sealed_module::Sealed<#trait_args> #trait_where_clause {
            #dispatch_trait_items
        }
        #(#impls)*
    }
}

fn prepare_dispatch_wrapper_sig(
    sig: &syn::Signature,
    dispatch_generics: &syn::Generics,
    co3: &TokenStream,
) -> syn::Signature {
    let mut wrapper_sig = prepare_dispatch_forwarding_sig(sig);

    let dispatch_tys = dispatch_type_idents(dispatch_generics);
    let parameter_idents = dispatch_generics
        .type_params()
        .map(|param| param.ident.clone())
        .collect::<Vec<_>>();
    let where_clause = wrapper_sig.generics.make_where_clause();

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
    let parameter_detector = ParamUseDetector::new(parameter_idents.iter());
    for input in &sig.inputs {
        let FnArg::Typed(input) = input else { continue };
        let (attrs, ty) = (&input.attrs[..], &*input.ty);
        let parameterized_spread =
            ffi_fn::is_spread_arg(attrs) && parameter_detector.type_mentions_param(ty);
        if parameterized_spread {
            let spread_ty = ffi_fn::item_fn_input_arg_type(attrs, ty);
            let (part1, part2) = ffi_fn::spread_types(attrs)
                .expect("validated #[spread] attribute")
                .expect("spread attribute was found");
            where_clause
                .predicates
                .push(syn::parse_quote!(#spread_ty: #co3::slice::Spread2));
            let try_spread = ffi_fn::is_try_spread_arg(attrs);
            if matches!(part1, syn::Type::Infer(_)) {
                where_clause.predicates.push(syn::parse_quote!(
                    <#spread_ty as #co3::slice::Spread2>::Part1: #co3::CFnArg
                ));
            } else {
                where_clause
                    .predicates
                    .push(syn::parse_quote!(#part1: #co3::ExternC));
                where_clause.predicates.push(syn::parse_quote!(
                    <#part1 as #co3::ExternC>::CType: #co3::CFnArg
                ));
                where_clause.predicates.push(spread_conversion_bound(
                    &spread_ty, 1, &part1, try_spread, co3,
                ));
            }
            if matches!(part2, syn::Type::Infer(_)) {
                where_clause.predicates.push(syn::parse_quote!(
                    <#spread_ty as #co3::slice::Spread2>::Part2: #co3::CFnArg
                ));
            } else {
                where_clause
                    .predicates
                    .push(syn::parse_quote!(#part2: #co3::ExternC));
                where_clause.predicates.push(syn::parse_quote!(
                    <#part2 as #co3::ExternC>::CType: #co3::CFnArg
                ));
                where_clause.predicates.push(spread_conversion_bound(
                    &spread_ty, 2, &part2, try_spread, co3,
                ));
            }
        }
        if crate::dispatch::handle_id(ty).is_none()
            && (detector.type_mentions_param(ty) || parameterized_spread)
        {
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

fn prepare_dispatch_forwarding_sig(sig: &syn::Signature) -> syn::Signature {
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
    wrapper_sig
}

fn spread_conversion_bound(
    spread_ty: &TokenStream,
    part: u8,
    target: &syn::Type,
    try_spread: bool,
    co3: &TokenStream,
) -> syn::WherePredicate {
    let part = syn::Ident::new(&format!("Part{part}"), proc_macro2::Span::call_site());
    if try_spread {
        syn::parse_quote!(
            <#spread_ty as #co3::slice::Spread2>::#part:
                core::convert::TryInto<<#target as #co3::ExternC>::CType>
        )
    } else {
        syn::parse_quote!(
            <#spread_ty as #co3::slice::Spread2>::#part:
                core::convert::Into<<#target as #co3::ExternC>::CType>
        )
    }
}

fn dispatch_id_assignments(sig: &syn::Signature, self_ty: Option<&syn::Type>) -> Vec<TokenStream> {
    sig.inputs
        .iter()
        .filter_map(|input| {
            let FnArg::Typed(syn::PatType { pat, ty, .. }) = input else {
                return None;
            };
            match crate::dispatch::handle_id(ty)? {
                crate::dispatch::HandleId::DynType(ident) => {
                    Some(quote! { let #pat = <#ident as co3::handle::Handle>::ID; })
                }
                crate::dispatch::HandleId::DynSelf => {
                    let self_ty = self_ty?;
                    Some(quote! { let #pat = <#self_ty as co3::handle::Handle>::ID; })
                }
            }
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
        id_checks,
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
        #id_checks
        #layout_checks
        #extern_decl
        #(#id_assignments)*
        #self_binding
        #wrapper_body
    }
}

fn gen_dispatch_import_module(
    module_name: &syn::Ident,
    DispatchImportParts { set, .. }: &DispatchImportParts,
    definitions: TokenStream,
) -> TokenStream {
    if set.is_empty() && definitions.is_empty() {
        return TokenStream::new();
    }

    quote! {
        #[allow(unused_braces)]
        mod #module_name {
            use super::*;

            #set
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
    sig: &syn::Signature,
    dispatch_args: &DispatchGroups,
    set_name: &syn::Ident,
    enclosing_module: Option<&syn::Ident>,
    self_ty: Option<&syn::Type>,
    impl_generics: Option<&syn::Generics>,
    declared_self: bool,
    self_id: Option<&syn::Type>,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> DispatchImportParts {
    // Dynamic parameters remain generic in the wrapper. Explicit source lifetimes are shared
    // with the dispatch set; raw-signature normalization and additional anonymous dispatch
    // lifetimes belong only to the extern declaration synthesized below.
    let mut wrapper_source_sig = sig.clone();
    ffi_fn::explicitize_signature_lifetimes(&mut wrapper_source_sig);
    let dispatch_generics = combine_dispatch_generics(impl_generics, &wrapper_source_sig.generics);
    let mut extern_sig = sig.clone();
    normalize_fn_signature(&mut extern_sig, self_ty);
    let mut extern_args = dispatch_args.clone();
    extern_args.inject_unnamed_lifetimes(&mut extern_sig.generics);
    let mut extern_generics = extern_sig.generics.clone();
    if let Some(impl_generics) = impl_generics {
        merge_generics(impl_generics.clone(), &mut extern_generics);
    }
    let trait_args = dispatch_trait_args(&dispatch_generics);
    let dispatch_set_path =
        enclosing_module.map_or_else(|| quote!(#set_name), |module| quote!(#module::#set_name));
    let delegate_sig = prepare_dispatch_forwarding_sig(&wrapper_source_sig);
    let mut wrapper_sig = delegate_sig.clone();
    wrapper_sig.generics.make_where_clause().predicates.insert(
        0,
        dispatch_membership_bound(&dispatch_set_path, &trait_args),
    );
    if let Some(impl_generics) = impl_generics
        && !declared_self
        && extern_sig
            .inputs
            .iter()
            .any(|input| matches!(input, FnArg::Receiver(_)))
    {
        merge_generics(impl_generics.clone(), &mut extern_sig.generics);
    }
    strip_erased_param_predicates(&extern_generics, &mut extern_sig.generics);
    let receiver = self_ty.map_or(crate::dispatch::DispatchReceiver::None, |ty| {
        if declared_self {
            crate::dispatch::DispatchReceiver::DynSelf { ty, id: self_id }
        } else {
            crate::dispatch::DispatchReceiver::Impl { ty, id: None }
        }
    });
    erase_dispatch_signature(&extern_generics, receiver, &mut extern_sig);
    strip_dispatch_params(&mut extern_sig.generics);
    let raw_sig = ffi_fn::lower_extern_fn_signature(extern_sig, failure_mode);
    let sealed_module = format_ident!("sealed");
    let delegate_name = &wrapper_source_sig.ident;
    let delegate_callee = quote!(<() as #dispatch_set_path<#trait_args>>::#delegate_name);
    let wrapper_args = dispatch_source_arg_names(&wrapper_source_sig);
    let wrapper_body = quote!(#delegate_callee(#(#wrapper_args),*));
    let delegate = DispatchSetDelegate {
        abi,
        block_attrs,
        fn_attrs,
        failure_mode,
        source_sig: &wrapper_source_sig,
        wrapper_sig: &delegate_sig,
        raw_sig: &raw_sig,
        self_ty,
        dispatch_generics: &dispatch_generics,
        declared_self,
        symbol_fragments,
    };

    DispatchImportParts {
        set: gen_dispatch_set(
            set_name,
            &sealed_module,
            &dispatch_generics,
            dispatch_args,
            Some(&delegate),
        ),
        id_checks: gen_handle_id_type_checks(&dispatch_generics, dispatch_args),
        layout_checks: gen_dispatch_erased_layout_checks(&dispatch_generics, sig, dispatch_args),
        wrapper_sig,
        id_assignments: Vec::new(),
        wrapper_body,
        extern_decl: TokenStream::new(),
    }
}

#[expect(clippy::too_many_arguments)]
fn prepare_dynamic_dispatch_import(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    block_attrs: &[syn::Attribute],
    fn_attrs: &[syn::Attribute],
    sig: &syn::Signature,
    dispatch_args: &DispatchGroups,
    self_ty: Option<&syn::Type>,
    impl_generics: Option<&syn::Generics>,
    declared_self: bool,
    self_id: Option<&syn::Type>,
) -> DispatchImportParts {
    let mut wrapper_source_sig = sig.clone();
    ffi_fn::explicitize_signature_lifetimes(&mut wrapper_source_sig);
    let mut args = dispatch_args.clone();
    args.inject_unnamed_lifetimes(&mut wrapper_source_sig.generics);
    let dispatch_generics = combine_dispatch_generics(impl_generics, &wrapper_source_sig.generics);
    let co3 = co3_path();
    let wrapper_sig = prepare_dispatch_wrapper_sig(&wrapper_source_sig, &dispatch_generics, &co3);
    let id_assignments = dispatch_id_assignments(&wrapper_source_sig, self_ty);
    let dummy_self_ty = syn::parse_quote!(());
    let wrapper_body = gen_wrapper_body::<true>(
        failure_mode,
        fn_attrs.iter().any(ffi_fn::is_by_val_attr),
        Some(self_ty.unwrap_or(&dummy_self_ty)),
        Some(&dispatch_generics),
        declared_self,
        &wrapper_source_sig,
    );

    let mut extern_sig = wrapper_source_sig.clone();
    normalize_fn_signature(&mut extern_sig, self_ty);
    if let Some(impl_generics) = impl_generics
        && !declared_self
        && extern_sig
            .inputs
            .iter()
            .any(|input| matches!(input, FnArg::Receiver(_)))
    {
        merge_generics(impl_generics.clone(), &mut extern_sig.generics);
    }
    strip_erased_param_predicates(&dispatch_generics, &mut extern_sig.generics);
    let receiver = self_ty.map_or(crate::dispatch::DispatchReceiver::None, |ty| {
        if declared_self {
            crate::dispatch::DispatchReceiver::DynSelf { ty, id: self_id }
        } else {
            crate::dispatch::DispatchReceiver::Impl { ty, id: None }
        }
    });
    erase_dispatch_signature(&dispatch_generics, receiver, &mut extern_sig);
    strip_dispatch_params(&mut extern_sig.generics);
    let decl = gen_extern_fn_signature(extern_sig, failure_mode);

    DispatchImportParts {
        set: TokenStream::new(),
        id_checks: gen_handle_id_type_checks(&dispatch_generics, &args),
        layout_checks: gen_dispatch_erased_layout_checks(
            &dispatch_generics,
            &wrapper_source_sig,
            &args,
        ),
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
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> TokenStream {
    let co3 = co3_path();

    let exports = decls.into_iter().map(|decl| {
        let export = match decl {
        ForeignItem::Static(item) => return crate::statics::gen_export_static(item),
        ForeignItem::Type(ForeignItemType {
            ty,
            id,
            id_value,
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
                gen_export_impl(&abi, failure_mode, impl_, id.as_deref(), true, symbol_fragments)
            });

            let drop_impl = drop.as_ref().map(|drop| {
                let mut item = drop.item.clone();
                if trait_object_single_trait_bound(&drop.self_ty).is_some() {
                    materialize_dyn_self_receiver(&mut item);
                }
                item
            });

            let drop_check = drop_impl
                .as_ref()
                .map(|drop_impl| gen_drop_impl_check(&ty, drop_impl));
            let drop = drop.map(|item| {
                gen_export_impl(
                    &abi,
                    failure_mode,
                    item,
                    id.as_deref(),
                    true,
                    symbol_fragments,
                )
            });

            let opaque = derive_opaque_item(
                id.as_deref(),
                ident,
                &ty.generics,
                // TODO: This is not correct, but I don't think it matters whether it's ZST or not
                quote! { co3::rust_spec::size::Sized<co3::rust_spec::Gt<rust_spec::Zero>> },
                quote! { co3::rust_spec::niche::WithoutNiche },
            );
            let handle_impl = id_value
                .as_deref()
                .map(|value| gen_handle_impl(ident, &ty.generics, value));
            let size_check = gen_non_zst_sized_check(ident, &ty.generics);

            quote! {
                #(#type_cfg_attrs)*
                const _: () = {
                    #opaque
                    #handle_impl

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
        ForeignItem::Fn(mut item) => {
            normalize_fn_signature(&mut item.sig, None);
            let bindings = monomorphize_static_fn_bindings(item, symbol_fragments);
            let has_multiple_bindings = bindings.len() > 1;
            let definitions = bindings
                .into_iter()
                .enumerate()
                .map(|(index, (mut binding, callee))| {
                    if has_multiple_bindings {
                        let source_name = &binding.sig.ident;
                        binding.sig.ident = format_ident!("__co3_export_{source_name}_{index}");
                    }

                    if binding.dispatch_args.is_empty() {
                        ffi_fn::gen_fn_definition(&abi, failure_mode, binding.item, callee)
                    } else {
                        gen_dispatch_fn_export(&abi, failure_mode, binding, callee)
                    }
                });
            quote!(#(#definitions)*)
        }
        ForeignItem::Impl(impl_) => {
            gen_export_impl(&abi, failure_mode, impl_, None, false, symbol_fragments)
        }
    };

        quote! { const _: () = { use #co3 as co3; #export }; }
    });

    quote! { #(#exports)* }
}

#[expect(clippy::too_many_arguments)]
fn synthesize_impl_extern_decls(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    attrs: &[syn::Attribute],
    impl_: ItemImpl,
    self_id: Option<&syn::Type>,
    args: Option<&DispatchGroups>,
    declared_self: bool,
    selection: Option<&[crate::DispatchSelection<'_>]>,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> Vec<TokenStream> {
    let dispatch_generics = impl_.generics.clone();
    let receiver =
        crate::dispatch::DispatchReceiver::for_impl(&impl_.self_ty, &impl_.self_ty, self_id);
    impl_
        .items
        .into_iter()
        .filter_map(|item| {
            let syn::ImplItem::Fn(mut item) = item else {
                return None;
            };

            let has_receiver = item
                .sig
                .inputs
                .iter()
                .any(|input| matches!(input, FnArg::Receiver(_)));
            normalize_fn_signature(&mut item.sig, Some(&impl_.self_ty));
            if declared_self {
                erase_dispatch_signature(
                    &syn::Generics::default(),
                    crate::dispatch::DispatchReceiver::DynSelf {
                        ty: &impl_.self_ty,
                        id: None,
                    },
                    &mut item.sig,
                );
            }
            if args.is_some() {
                merge_generics(impl_.generics.clone(), &mut item.sig.generics);
            } else {
                ffi_fn::merge_impl_generics_for_raw_decl(
                    impl_.generics.clone(),
                    has_receiver && !declared_self,
                    &mut item.sig.generics,
                );
            }

            let erased_layout_checks = args
                .map(|args| gen_dispatch_erased_layout_checks(&impl_.generics, &item.sig, args));

            if args.is_some() {
                strip_erased_param_predicates(&impl_.generics, &mut item.sig.generics);
                erase_dispatch_signature(&impl_.generics, receiver, &mut item.sig);
                strip_dispatch_params(&mut item.sig.generics);
            }
            if let Some(selection) = selection {
                // Direct dynamic parameters have already been erased. This
                // substitution resolves the concrete wrapper/self types and
                // projections which deliberately survive erasure.
                DispatchMonomorphizer::for_dispatch_group(&dispatch_generics, selection)
                    .visit_signature_mut(&mut item.sig);
                DispatchMonomorphizer::for_static_dispatch_group(&dispatch_generics, selection)
                    .interpolate_symbol_attrs(&mut item.attrs, symbol_fragments);
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

pub(crate) fn expand_extern_decls(
    abi: syn::Abi,
    features: MacroFeatures,
    failure_mode: FailureMode,
    attrs: &[syn::Attribute],
    decls: Vec<ForeignItem>,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> TokenStream {
    fn expand_impl_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        impl_: ItemImpl,
        declared_self: bool,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    ) -> TokenStream {
        let co3 = co3_path();
        let import = wrap_impl_definition::<false>(failure_mode, &impl_, declared_self);
        let extern_decl = synthesize_impl_extern_decls(
            abi,
            failure_mode,
            attrs,
            impl_,
            None,
            None,
            declared_self,
            None,
            symbol_fragments,
        );

        quote! {
            const _: () = {
                use #co3 as co3;
                #(#extern_decl)*
                #import
            };
        }
    }

    #[expect(clippy::too_many_arguments)]
    fn synthesize_dispatched_impl_imports(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        source_impl: &ItemImpl,
        args: &DispatchGroups,
        self_id: Option<&syn::Type>,
        declared_self: bool,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    ) -> TokenStream {
        let mut wrapper_source = source_impl.clone();
        if declared_self {
            materialize_dyn_self_receiver(&mut wrapper_source);
        }
        let wrapper_impl =
            wrap_impl_definition::<true>(failure_mode, &wrapper_source, declared_self);

        let mut imports = Vec::new();
        args.for_each_combination(|selections| {
            let extern_decls = synthesize_impl_extern_decls(
                abi,
                failure_mode,
                attrs,
                source_impl.clone(),
                self_id,
                Some(args),
                false,
                Some(selections),
                symbol_fragments,
            );
            let mut concrete_wrapper = wrapper_impl.clone();
            DispatchMonomorphizer::for_dispatch_group(&source_impl.generics, selections)
                .visit_item_impl_mut(&mut concrete_wrapper);
            concrete_wrapper.generics.params =
                core::mem::take(&mut concrete_wrapper.generics.params)
                    .into_iter()
                    .filter(|param| match param {
                        syn::GenericParam::Lifetime(_) => true,
                        syn::GenericParam::Type(param) => !args.contains_param(&param.ident),
                        syn::GenericParam::Const(param) => !args.contains_param(&param.ident),
                    })
                    .collect();
            imports.push(quote! {
                const _: () = {
                    #(#extern_decls)*
                    #concrete_wrapper
                };
            });
        });

        quote!(#(#imports)*)
    }

    fn synthesize_impl_dispatch_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        self_id: Option<&syn::Type>,
        dispatch: Co3Impl,
        declared_self: bool,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    ) -> TokenStream {
        let Co3Impl {
            item: mut impl_,
            dispatch_args,
            ..
        } = dispatch;
        let mut args = dispatch_args;
        args.inject_unnamed_lifetimes(&mut impl_.generics);

        let dispatch_helper = gen_dispatch_helper(&impl_.generics, &args);
        let id_checks = gen_handle_id_type_checks(&impl_.generics, &args);
        let mut check_generics = impl_.generics.clone();
        strip_erased_param_predicates(&impl_.generics, &mut check_generics);
        check_generics.params = core::mem::take(&mut check_generics.params)
            .into_iter()
            .filter(|param| match param {
                syn::GenericParam::Lifetime(_) => true,
                syn::GenericParam::Type(param) => !args.contains_param(&param.ident),
                syn::GenericParam::Const(param) => !args.contains_param(&param.ident),
            })
            .map(|mut param| {
                match &mut param {
                    syn::GenericParam::Type(param) => param.default = None,
                    syn::GenericParam::Const(param) => param.default = None,
                    syn::GenericParam::Lifetime(_) => {}
                }
                param
            })
            .collect();
        let checks = if has_non_lifetime_generics(&check_generics) {
            let (check_impl_generics, _, check_where_clause) = check_generics.split_for_impl();
            quote! {
                fn __co3_check_dispatch #check_impl_generics () #check_where_clause {
                    #dispatch_helper
                    #id_checks
                }
            }
        } else {
            quote! {
                #dispatch_helper
                #id_checks
            }
        };
        let imports = synthesize_dispatched_impl_imports(
            abi,
            failure_mode,
            attrs,
            &impl_,
            &args,
            self_id,
            declared_self,
            symbol_fragments,
        );
        let co3 = co3_path();

        quote! {
            const _: () = {
                use #co3 as co3;
                #checks

                #imports
            };
        }
    }

    fn expand_dispatch_fn_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        mut dispatch: Co3Fn,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    ) -> TokenStream {
        let args = core::mem::take(&mut dispatch.dispatch_args);
        let syn::ItemFn {
            attrs: fn_attrs,
            sig,
            vis,
            ..
        } = &mut *dispatch;
        let dispatch_set = dispatch_set_name();
        let module_name = sig.ident.clone();
        let parts = if has_static_dispatch(&sig.generics, &args) {
            prepare_dispatch_import(
                abi,
                failure_mode,
                attrs,
                fn_attrs,
                sig,
                &args,
                &dispatch_set,
                Some(&module_name),
                None,
                None,
                false,
                None,
                symbol_fragments,
            )
        } else {
            prepare_dynamic_dispatch_import(
                abi,
                failure_mode,
                attrs,
                fn_attrs,
                sig,
                &args,
                None,
                None,
                false,
                None,
            )
        };
        let wrapper_attrs = fn_attrs
            .iter()
            .filter(|attr| !crate::is_symbol_name_attr(attr) && !ffi_fn::is_by_val_attr(attr));
        let vis = &*vis;
        let wrapper_sig = &parts.wrapper_sig;
        let wrapper_body = gen_dispatch_import_wrapper_body(&parts, TokenStream::new());
        let module = gen_dispatch_import_module(&module_name, &parts, TokenStream::new());
        let module_attrs = (!module.is_empty())
            .then(|| cfg_attrs(fn_attrs))
            .into_iter()
            .flatten();

        quote! {
            #(#module_attrs)*
            #module

            #(#wrapper_attrs)*
            #vis #wrapper_sig {
                #wrapper_body
            }
        }
    }

    fn expand_dispatch_method_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        mut dispatch: Co3Impl,
        declared_self: bool,
        self_id: Option<&syn::Type>,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    ) -> TokenStream {
        let args = core::mem::take(&mut dispatch.dispatch_args);
        let syn::ImplItem::Fn(mut method) = dispatch.items.pop().unwrap() else {
            unreachable!()
        };
        move_method_only_impl_params(&mut dispatch.item, &mut method);
        if declared_self {
            materialize_dyn_self_receiver(&mut dispatch.item);
        }
        let self_ty = &dispatch.self_ty;
        let dispatch_set = dispatch_set_name();
        let module_name = dispatch_module_name(self_ty, &method.sig.ident);
        let static_dispatch = has_static_dispatch(&method.sig.generics, &args)
            || has_static_dispatch(&dispatch.generics, &args);
        let parts = if static_dispatch {
            prepare_dispatch_import(
                abi,
                failure_mode,
                attrs,
                &method.attrs,
                &method.sig,
                &args,
                &dispatch_set,
                Some(&module_name),
                Some(self_ty),
                Some(&dispatch.generics),
                declared_self,
                self_id,
                symbol_fragments,
            )
        } else {
            prepare_dynamic_dispatch_import(
                abi,
                failure_mode,
                attrs,
                &method.attrs,
                &method.sig,
                &args,
                Some(self_ty),
                Some(&dispatch.generics),
                declared_self,
                self_id,
            )
        };
        let wrapper_attrs = method
            .attrs
            .iter()
            .filter(|attr| !crate::is_symbol_name_attr(attr) && !ffi_fn::is_by_val_attr(attr));
        let vis = &method.vis;
        let self_binding = method
            .sig
            .inputs
            .iter()
            .any(|input| matches!(input, FnArg::Receiver(_)))
            .then(|| quote!(let __co3_self = self;));
        let ItemImpl {
            attrs: impl_attrs,
            modifiers,
            unsafety,
            trait_,
            self_ty,
            ..
        } = &*dispatch;
        let trait_ = trait_
            .as_ref()
            .map(|(path, _)| quote!(#path for))
            .unwrap_or_default();
        let defaultness = &modifiers.defaultness;
        let mut wrapper_impl_generics = dispatch.generics.clone();
        wrapper_impl_generics
            .type_params_mut()
            .for_each(strip_internal_generic_param);
        let (impl_generics, _, where_clause) = wrapper_impl_generics.split_for_impl();
        let mut wrapper_sig = parts.wrapper_sig.clone();
        if let Some(position) = wrapper_sig
            .inputs
            .iter()
            .position(|input| matches!(input, FnArg::Receiver(_)))
            && position != 0
        {
            let mut inputs = core::mem::take(&mut wrapper_sig.inputs)
                .into_iter()
                .collect::<Vec<_>>();
            let receiver = inputs.remove(position);
            wrapper_sig.inputs = core::iter::once(receiver).chain(inputs).collect();
        }
        let wrapper_sig = &wrapper_sig;
        let wrapper_body =
            gen_dispatch_import_wrapper_body(&parts, self_binding.unwrap_or_default());

        let module = gen_dispatch_import_module(&module_name, &parts, TokenStream::new());

        quote! {
            #module

            #(#impl_attrs)*
            #[allow(unused_braces)]
            #defaultness #unsafety impl #impl_generics #trait_ #self_ty #where_clause {
                #(#wrapper_attrs)*
                #vis #wrapper_sig {
                    #wrapper_body
                }
            }
        }
    }

    fn expand_import_impl(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        impl_: Co3Impl,
        type_id: Option<&syn::Type>,
        declared_self: bool,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    ) -> TokenStream {
        let (mut plain, mut methods) = split_dyn_methods(impl_);
        let dyn_self = declared_self && trait_object_single_trait_bound(&plain.self_ty).is_some();
        if plain.trait_.is_none() && has_static_dispatch(&plain.generics, &plain.dispatch_args) {
            let mut retained = Vec::new();
            for item in core::mem::take(&mut plain.item.items) {
                let syn::ImplItem::Fn(method) = item else {
                    retained.push(item);
                    continue;
                };
                let mut method_impl = plain.item.clone();
                method_impl.items = vec![syn::ImplItem::Fn(method)];
                methods.push(Co3Impl {
                    item: method_impl,
                    dispatch_args: plain.dispatch_args.clone(),
                    method_dispatch_args: Default::default(),
                });
            }
            plain.item.items = retained;
        }
        let plain = monomorphize_static_impl_bindings(plain, symbol_fragments)
            .into_iter()
            .map(|descriptor| {
                if descriptor.items.is_empty() {
                    TokenStream::new()
                } else if !descriptor.dispatch_args.is_empty()
                    || has_payload_dispatch(&descriptor.generics)
                    || dyn_self
                {
                    let self_id = dyn_self.then_some(type_id).flatten();
                    synthesize_impl_dispatch_import(
                        abi,
                        failure_mode,
                        attrs,
                        self_id,
                        descriptor,
                        declared_self,
                        symbol_fragments,
                    )
                } else {
                    expand_impl_import(
                        abi,
                        failure_mode,
                        attrs,
                        descriptor.item,
                        declared_self,
                        symbol_fragments,
                    )
                }
            });
        let methods = methods.into_iter().map(|method| {
            let binds_impl_parameter = method.generics.params.iter().any(|param| match param {
                syn::GenericParam::Lifetime(_) => false,
                syn::GenericParam::Type(param) => {
                    !param.attrs.iter().any(is_type_erased)
                        && method.dispatch_args.contains_param(&param.ident)
                }
                syn::GenericParam::Const(param) => {
                    method.dispatch_args.contains_param(&param.ident)
                }
            });
            if binds_impl_parameter && method.trait_.is_some() {
                let self_id = dyn_self.then_some(type_id).flatten();
                let descriptors = monomorphize_static_impl_bindings(
                    lift_dispatch_method(method),
                    symbol_fragments,
                );
                let imports = descriptors.into_iter().map(|descriptor| {
                    synthesize_impl_dispatch_import(
                        abi,
                        failure_mode,
                        attrs,
                        self_id,
                        descriptor,
                        declared_self,
                        symbol_fragments,
                    )
                });
                quote!(#(#imports)*)
            } else {
                expand_dispatch_method_import(
                    abi,
                    failure_mode,
                    attrs,
                    method,
                    declared_self,
                    type_id,
                    symbol_fragments,
                )
            }
        });

        quote!(#(#plain)* #(#methods)*)
    }

    let imports = decls.into_iter().map(|decl| match decl {
        ForeignItem::Type(ForeignItemType {
            ty,
            id,
            id_value,
            self_impls,
            drop,
        }) => {
            let type_cfg_attrs = cfg_attrs(&ty.attrs)
                .map(|attr| quote!(#attr))
                .collect::<Vec<_>>();
            let ty = wrap_extern_type_decl(
                &abi,
                features,
                attrs,
                id.as_deref(),
                id_value.as_deref(),
                drop.as_ref()
                    .is_some_and(|item| trait_object_single_trait_bound(&item.self_ty).is_some()),
                ty,
            );

            let dispatch = self_impls.into_iter().map(|impl_| {
                expand_import_impl(
                    &abi,
                    failure_mode,
                    attrs,
                    impl_,
                    id.as_deref(),
                    true,
                    symbol_fragments,
                )
            });

            let drop = drop.map(|item| {
                if item.dispatch_args.is_empty()
                    && trait_object_single_trait_bound(&item.self_ty).is_none()
                {
                    expand_impl_import(&abi, failure_mode, attrs, item.item, true, symbol_fragments)
                } else {
                    expand_dispatch_drop_import(
                        &abi,
                        failure_mode,
                        attrs,
                        item,
                        id.as_deref(),
                        symbol_fragments,
                    )
                }
            });

            quote! {
                #ty

                #(#type_cfg_attrs)*
                #(#dispatch)*
                #drop
            }
        }
        ForeignItem::Fn(item) => {
            if !item.dispatch_args.is_empty() || has_payload_dispatch(&item.sig.generics) {
                expand_dispatch_fn_import(&abi, failure_mode, attrs, item, symbol_fragments)
            } else {
                wrap_fn_definition(&abi, failure_mode, attrs, item.item)
            }
        }
        ForeignItem::Impl(impl_) => expand_import_impl(
            &abi,
            failure_mode,
            attrs,
            impl_,
            None,
            false,
            symbol_fragments,
        ),
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

fn gen_handle_impl(ident: &syn::Ident, generics: &syn::Generics, value: &syn::Expr) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        unsafe impl #impl_generics co3::handle::Handle for #ident #ty_generics #where_clause {
            const ID: Self::Kind = #value;
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
        quote! {
            const _: () = assert!(
                core::mem::size_of::<#ident #static_ty_generics>() != 0,
                concat!(stringify!(#ident), " must not be zero-sized")
            );
        }
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
    if args.is_empty() {
        return None;
    }

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
            let _: __Co3DispatchParams::<#(#erased_args),*> =
                __Co3DispatchParams(core::marker::PhantomData);
        });
    });
    Some(quote! {
        // NOTE: Verifies `?Sized` bounds on erased params
        struct __Co3DispatchParams<#(#erased_generics),*>(
            core::marker::PhantomData<(#(*const #erased_tys),*)>
        );

        #(#dispatch_checks)*
    })
}

fn expand_dispatch_drop_import(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    attrs: &[syn::Attribute],
    item: Co3Impl,
    declared_id_ty: Option<&syn::Type>,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> TokenStream {
    let Co3Impl {
        item: source_impl,
        dispatch_args,
        ..
    } = item;
    let extern_decls = synthesize_impl_extern_decls(
        abi,
        failure_mode,
        attrs,
        source_impl.clone(),
        declared_id_ty,
        Some(&dispatch_args),
        false,
        None,
        symbol_fragments,
    );

    let declared_self = trait_object_single_trait_bound(&source_impl.self_ty).is_some();
    let mut impl_ = source_impl;
    materialize_dyn_self_receiver(&mut impl_);

    let ItemImpl {
        attrs: impl_attrs,
        generics,
        self_ty,
        items,
        ..
    } = &impl_;

    let mut wrapper_generics = generics.clone();
    wrapper_generics
        .type_params_mut()
        .for_each(strip_internal_generic_param);
    let (impl_generics, _, _) = wrapper_generics.split_for_impl();
    let predicates = wrapper_generics
        .where_clause
        .as_ref()
        .map(|w| &w.predicates);

    let ImplItem::Fn(method) = items.iter().next().unwrap() else {
        unreachable!()
    };
    let wrapper_attrs = method
        .attrs
        .iter()
        .filter(|attr| !crate::is_symbol_name_attr(attr) && !ffi_fn::is_by_val_attr(attr));
    let self_handle_bound = method.sig.inputs.iter().any(|input| {
        matches!(input, FnArg::Typed(input)
            if matches!(crate::dispatch::handle_id(&input.ty), Some(crate::dispatch::HandleId::DynSelf)))
    });
    let self_handle_bound = self_handle_bound.then(|| quote!(Self: co3::handle::Handle,));

    let mut lowered_method = method.clone();
    lowered_method.sig.output = syn::ReturnType::Default;
    let selector_assignments = lowered_method
        .sig
        .inputs
        .iter_mut()
        .filter_map(|input| {
            let FnArg::Typed(syn::PatType { pat, ty, .. }) = input else {
                return None;
            };
            let (handle_ty, id_ty) = match crate::dispatch::handle_id(ty)? {
                crate::dispatch::HandleId::DynType(ident) => {
                    let param = generics.type_params().find(|param| param.ident == *ident)?;
                    (quote!(#ident), erased_id_repr(param)?.clone())
                }
                crate::dispatch::HandleId::DynSelf => (quote!(Self), declared_id_ty?.clone()),
            };
            **ty = id_ty.clone();
            Some(quote! {
                let #pat: #id_ty = {
                    // FIXME: https://github.com/mversic/co3/issues/93
                    // Should it be required that HandleFamily::Kind: Copy
                    let __co3_handle_id = <#handle_ty as co3::handle::Handle>::ID;
                    unsafe { core::mem::transmute_copy(&__co3_handle_id) }
                };
            })
        })
        .collect::<Vec<_>>();
    let body = gen_impl_wrapper_body::<true>(
        failure_mode,
        &lowered_method,
        self_ty,
        generics,
        declared_self,
    );
    let co3 = co3_path();

    quote! {
        const _: () = {
            use #co3 as co3;
            #(#extern_decls)*

            #(#impl_attrs)*
            impl #impl_generics Drop for #self_ty where
                #self_handle_bound
                #predicates
            {
                #(#wrapper_attrs)*
                fn drop(&mut self) {
                    #(#selector_assignments)*
                    #body
                }
            }
        };
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
    id_value: Option<&syn::Expr>,
    has_dyn_self_drop: bool,
    mut type_: syn::ForeignItemType,
) -> TokenStream {
    // A `dyn Self` drop dispatches through the opaque type's Handle
    // implementation, so generic opaque types need the bound on their
    // declaration. Other drops, including the generated Owned wrapper drop,
    // do not require it.
    if has_non_lifetime_generics(&type_.generics) && has_dyn_self_drop && id_value.is_none() {
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
    let ident_impls = gen_extern_type_impls(id, id_value, &ident, &generics);
    let owned_impls = gen_owned_extern_type_impls(id, id_value, &ident, &generics);
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
    id_value: Option<&syn::Expr>,
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
    let handle_impl = id_value.map(|value| gen_handle_impl(ident, generics, value));

    quote! {
        #opaque_impls
        #handle_impl

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

fn gen_owned_extern_type_impls(
    id: Option<&syn::Type>,
    id_value: Option<&syn::Expr>,
    ident: &syn::Ident,
    generics: &syn::Generics,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut decode_generics = generics.clone();
    decode_generics.params.insert(0, syn::parse_quote!('d));
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();

    let owned_ident = gen_owned_extern_type_name(ident);
    let owned_repr_c_name = gen_owned_repr_c_name(ident);
    let handle_family_impl = id
        .map(|id| gen_handle_family_impl(&owned_ident, generics, id))
        .unwrap_or_default();
    let handle_impl = id_value.map(|value| gen_handle_impl(&owned_ident, generics, value));

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

        #handle_family_impl
        #handle_impl

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
