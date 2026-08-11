use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    Ident, Path, Type, parse_quote,
    visit::{Visit, visit_type},
    visit_mut::VisitMut,
};

use crate::{
    dispatch::{StaticLifetimeNormalizer, handle_id},
    generate::OwnershipMode,
    parse::FailureMode,
    symbol_name_value,
    utils::soft_for_arg,
};

fn export_definition_attrs(attrs: &[syn::Attribute]) -> TokenStream {
    let attrs = attrs.iter().filter_map(|attr| {
        if is_by_val_attr(attr) {
            return None;
        }
        let Some(value) = symbol_name_value(attr) else {
            return Some(quote!(#attr));
        };

        Some(quote!(#[unsafe(export_name = #value)]))
    });

    quote!(#(#attrs)*)
}

pub(crate) fn is_by_val_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("by_val")
}

pub(crate) fn fn_return_ty(sig: &syn::Signature) -> Option<&syn::Type> {
    match &sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, ty) => Some(ty),
    }
}

pub(crate) fn gen_failure_panic(err: TokenStream) -> TokenStream {
    quote! {
        panic!(
            "co3 generated FFI failure with error type `{}`",
            core::any::type_name_of_val(&#err),
        )
    }
}

pub(crate) fn gen_trap_value() -> TokenStream {
    quote! { co3::Error::trap_value() }
}

pub(crate) fn gen_soft_sync_error_value() -> TokenStream {
    quote! { co3::Error::soft_sync_error() }
}

pub(crate) fn gen_decode_error(failure_mode: FailureMode) -> TokenStream {
    match failure_mode {
        FailureMode::Panic => quote! { panic!("co3 generated FFI decode failure"); },
        FailureMode::Error => {
            let error = gen_trap_value();
            quote! { return Err(#error); }
        }
    }
}

pub(crate) fn gen_sync_error(failure_mode: FailureMode) -> TokenStream {
    match failure_mode {
        FailureMode::Panic => quote! { panic!("co3 generated FFI sync failure"); },
        FailureMode::Error => {
            let error = gen_soft_sync_error_value();
            quote! { return Err(#error); }
        }
    }
}

pub(crate) fn gen_sync_check(
    store_sync_stmts: TokenStream,
    sync_error: TokenStream,
) -> TokenStream {
    quote! {
        let __co3_sync_errors = #store_sync_stmts;
        let mut __co3_sync_errors_iter = core::iter::IntoIterator::into_iter(__co3_sync_errors);
        if core::iter::Iterator::any(&mut __co3_sync_errors_iter, core::convert::identity) {
            #sync_error
        }
    }
}

pub(crate) fn gen_unknown_handle_error(failure_mode: FailureMode) -> TokenStream {
    match failure_mode {
        FailureMode::Panic => quote! { panic!("co3 generated FFI unknown handle") },
        FailureMode::Error => quote! { Err(co3::Error::unknown_handle()) },
    }
}

pub(crate) fn emit_extern_definition(
    abi: &syn::Abi,
    attrs: &[syn::Attribute],
    failure_mode: FailureMode,
    fn_signature: TokenStream,
    ffi_fn_body: TokenStream,
) -> TokenStream {
    let attrs = export_definition_attrs(attrs);

    let error_handler = match failure_mode {
        FailureMode::Panic => gen_failure_panic(quote!(err)),
        FailureMode::Error => quote! { co3::encode(err) },
    };

    quote! {
        #attrs
        unsafe #abi #fn_signature {
            let fn_body = || #ffi_fn_body;

            match fn_body() {
                Ok(value) => value,
                Err(err) => #error_handler,
            }
        }
    }
}

pub(crate) fn gen_definition_body(
    sig: syn::Signature,
    callee: TokenStream,
    fn_by_val: bool,
    failure_mode: FailureMode,
) -> TokenStream {
    let inputs = &sig.inputs;
    let return_ty = fn_return_ty(&sig);

    let decode_input_stmts = gen_input_decode_stmts(inputs, failure_mode);
    let store_sync_stmts = gen_store_sync_stmts(inputs.len());
    let sync_error = gen_sync_error(failure_mode);
    let sync_check = gen_sync_check(store_sync_stmts, sync_error);

    let arg_names = inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Typed(arg) => item_fn_input_ident(&arg.pat).clone(),
            syn::FnArg::Receiver(_) => format_ident!("__co3_self"),
        })
        .collect::<Vec<_>>();

    let return_borrow_check = if let Some(return_ty) = &return_ty {
        gen_return_borrow_check(return_ty, fn_by_val)
    } else {
        quote! {}
    };
    let output = match failure_mode {
        FailureMode::Panic => quote! { Ok::<_, ()>(co3::encode(__co3_output)) },
        FailureMode::Error => match return_ty {
            Some(return_ty) => quote! { Ok::<_, #return_ty>(co3::encode(__co3_output)) },
            None => quote! { Ok(co3::encode(__co3_output)) },
        },
    };

    quote! {{
        #return_borrow_check

        #decode_input_stmts
        let __co3_output = #callee(
            #(#arg_names),*
        );

        #sync_check

        #output
    }}
}

pub(crate) fn gen_return_borrow_check(return_ty: &syn::Type, fn_by_val: bool) -> TokenStream {
    if fn_by_val {
        return quote! {};
    }

    quote! {
        const {
            assert!(
                co3::impls!(#return_ty: co3::borrow::Borrow<Owner: co3::stored::EmptyStore>),
                "Use `move fn` to transfer ownership",
            );
        }
    }
}

pub(crate) fn item_fn_input_ident(input: &syn::Pat) -> &Ident {
    let syn::Pat::Ident(syn::PatIdent { ident, .. }) = input else {
        unreachable!()
    };

    ident
}

pub(crate) fn is_spread_arg(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("spread"))
}

fn borrowed_arg_ty(attrs: &[syn::Attribute], arg_ty: &Type) -> TokenStream {
    match ownership_mode_for_arg(attrs) {
        OwnershipMode::ByValue => quote!(#arg_ty),
        OwnershipMode::Borrow => quote! {
            <#arg_ty as co3::borrow::Borrow>::Borrowed<'_>
        },
    }
}

pub(crate) fn spread_arg_names(arg_name: &Ident) -> (Ident, Ident) {
    (
        format_ident!("__co3_{arg_name}_data"),
        format_ident!("__co3_{arg_name}_metadata"),
    )
}

pub(crate) fn gen_input_decode_stmts<'a>(
    inputs: impl IntoIterator<Item = &'a syn::FnArg>,
    failure_mode: FailureMode,
) -> TokenStream {
    let inputs = inputs.into_iter().collect::<Vec<_>>();

    let mut stmts = quote! {};
    let arg_names = inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Typed(arg) => item_fn_input_ident(&arg.pat).clone(),
            syn::FnArg::Receiver(_) => format_ident!("__co3_self"),
        })
        .collect::<Vec<_>>();

    let store_sync_stmts = gen_store_sync_stmts(inputs.len());
    let decode_error = gen_decode_error(failure_mode);

    let mut value_tys = Vec::with_capacity(inputs.len());
    let mut store_tys = Vec::with_capacity(inputs.len());

    let mut value_present = Vec::new();
    for (idx, input) in inputs.iter().enumerate() {
        let idx = syn::Index::from(idx);

        let (attrs, arg_name, arg_ty) = match input {
            syn::FnArg::Receiver(receiver) => (
                &receiver.attrs,
                format_ident!("__co3_self"),
                (*receiver.ty).clone(),
            ),
            syn::FnArg::Typed(arg) => {
                let arg_name = item_fn_input_ident(&arg.pat);
                let arg_ty = (*arg.ty).clone();
                (&arg.attrs, arg_name.clone(), arg_ty)
            }
        };

        let decode_ty = borrowed_arg_ty(attrs, &arg_ty);
        let decode_arg = if is_spread_arg(attrs) {
            let (data_name, metadata_name) = spread_arg_names(&arg_name);
            let decode_c_ty = quote!(<#decode_ty as co3::ExternC>::CType);

            quote! {{
                <#decode_c_ty as co3::slice::Spread2>::from_parts(
                    #data_name, #metadata_name
                )
            }}
        } else {
            quote! { #arg_name }
        };

        let decode_call = if soft_for_arg(attrs) {
            quote! { co3::soft_decode(#decode_arg, &mut __co3_input_stores.#idx) }
        } else {
            quote! { co3::decode(#decode_arg) }
        };

        let from_borrow = match ownership_mode_for_arg(attrs) {
            OwnershipMode::ByValue => quote!(#arg_name),
            OwnershipMode::Borrow => quote! {
                #arg_name.map(co3::borrow::FromBorrow::from_borrow)
            },
        };

        stmts.extend(quote! {
            let #arg_name: Option<#decode_ty> = unsafe { #decode_call };
            let #arg_name: Option<#arg_ty> = #from_borrow;

            if let Some(#arg_name) = #arg_name {
                __co3_input_values.#idx = Some(#arg_name);
            }
        });

        value_tys.push(arg_ty);
        store_tys.push(if soft_for_arg(attrs) {
            quote!(<#decode_ty as co3::stored::DecodeOwned<'_>>::Store)
        } else {
            quote!(())
        });

        value_present.push(quote! { __co3_input_values.#idx.is_some() });
    }

    quote! {
        let mut __co3_input_values = (#(<Option<#value_tys> as core::default::Default>::default(),)*);
        let mut __co3_input_stores = (#(<#store_tys as core::default::Default>::default(),)*);

        #stmts

        let __co3_input_present: [bool; _] = [
            #(#value_present),*
        ];

        let (#(Some(#arg_names),)*) = __co3_input_values else {
            let __co3_sync_errors = #store_sync_stmts;
            #decode_error
        };
    }
}

pub(crate) fn gen_fn_signature_check(
    mut sig: syn::Signature,
    mut callee: syn::Expr,
) -> TokenStream {
    StaticLifetimeNormalizer.visit_signature_mut(&mut sig);
    StaticLifetimeNormalizer.visit_expr_mut(&mut callee);

    let syn::Signature {
        unsafety,
        abi,
        output,
        inputs,
        ..
    } = &sig;

    let arg_tys = inputs.iter().map(|input| match input {
        syn::FnArg::Receiver(syn::Receiver { ty, .. }) => ty,
        syn::FnArg::Typed(syn::PatType { ty, .. }) => ty,
    });

    let fn_ty = quote! { #unsafety #abi fn(#(#arg_tys),*) #output };

    // NOTE: Avoids signature drift
    quote! { let __co3_fn: #fn_ty = #callee; }
}

pub(crate) fn gen_store_sync_stmts(len: usize) -> TokenStream {
    let idxs = (0..len).map(syn::Index::from);

    quote! {{
        let mut __co3_sync_errors = [false; #len]; #(

        if __co3_input_present[#idxs] && co3::stored::Store::sync(__co3_input_stores.#idxs).is_none() {
            __co3_sync_errors[#idxs] = true;
        })*

        __co3_sync_errors
    }}
}

fn gen_fn_definition_body(item: &syn::ItemFn, failure_mode: FailureMode) -> TokenStream {
    let fn_name = &item.sig.ident;
    let callee = quote! { self::#fn_name };
    let check_callee = parse_quote! { self::#fn_name };
    let fn_by_val = item.attrs.iter().any(is_by_val_attr);
    let signature_check = gen_fn_signature_check(item.sig.clone(), check_callee);
    let body = gen_definition_body(item.sig.clone(), callee, fn_by_val, failure_mode);

    quote! {{
        #signature_check
        #body
    }}
}

pub fn gen_impl_definition(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    mut impl_: syn::ItemImpl,
) -> TokenStream {
    let trait_ = impl_.trait_.as_ref().map(|(_, path, _)| path);
    let impl_attrs = &impl_.attrs;

    let self_ty = &impl_.self_ty;
    impl_.items.iter_mut().for_each(|item| {
        let syn::ImplItem::Fn(item) = item else {
            return;
        };

        normalize_fn_signature(&mut item.sig, Some(self_ty));
    });

    let definitions = impl_.items.into_iter().filter_map(|item| {
        let syn::ImplItem::Fn(mut item) = item else {
            return None;
        };

        let fn_name = &item.sig.ident;
        let callee = if let Some(trait_) = trait_ {
            quote!(<#self_ty as #trait_>::#fn_name)
        } else {
            quote!(<#self_ty>::#fn_name)
        };

        merge_generics(impl_.generics.clone(), &mut item.sig.generics);
        let check_callee = if let Some(trait_) = trait_ {
            parse_quote!(<#self_ty as #trait_>::#fn_name)
        } else {
            parse_quote!(<#self_ty>::#fn_name)
        };

        let fn_by_val = item.attrs.iter().any(is_by_val_attr);
        let fn_signature = gen_extern_fn_signature(item.sig.clone(), failure_mode);
        let signature_check = gen_fn_signature_check(item.sig.clone(), check_callee);
        let body = gen_definition_body(item.sig, callee, fn_by_val, failure_mode);

        let ffi_fn_body = quote! {{
            #signature_check
            #body
        }};

        Some(emit_extern_definition(
            abi,
            &item.attrs,
            failure_mode,
            fn_signature,
            ffi_fn_body,
        ))
    });

    quote! {
        #(#impl_attrs)*
        const _: () = {
            #(#definitions)*
        };
    }
}

pub(crate) fn merge_generics(impl_generics: syn::Generics, fn_generics: &mut syn::Generics) {
    fn_generics.params.extend(impl_generics.params);

    if let Some(impl_where_clause) = impl_generics.where_clause {
        fn_generics
            .make_where_clause()
            .predicates
            .extend(impl_where_clause.predicates);
    }
}

pub(crate) fn strip_dispatch_params(generics: &mut syn::Generics) {
    generics.params = core::mem::take(&mut generics.params)
        .into_iter()
        .filter(|param| matches!(param, syn::GenericParam::Lifetime(_)))
        .collect();
}

pub fn gen_fn_definition(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    mut item: syn::ItemFn,
) -> TokenStream {
    normalize_fn_signature(&mut item.sig, None);

    let ffi_fn_body = gen_fn_definition_body(&item, failure_mode);
    let fn_signature = gen_extern_fn_signature(item.sig, failure_mode);

    emit_extern_definition(abi, &item.attrs, failure_mode, fn_signature, ffi_fn_body)
}

pub(crate) fn gen_extern_fn_signature(
    mut sig: syn::Signature,
    failure_mode: FailureMode,
) -> TokenStream {
    if let syn::ReturnType::Type(_, return_type) = &sig.output
        && matches!(failure_mode, FailureMode::Error)
    {
        sig.generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#return_type: co3::Error));
    }

    explicitize_signature_lifetimes(&mut sig);
    // FIXME: This feels like a HACK
    synthesize_lifetime_bounds(&mut sig);

    lower_signature_inputs(&mut sig);
    lower_signature_output(&mut sig);

    sig.constness = None;
    sig.asyncness = None;
    sig.unsafety = None;
    sig.abi = None;

    quote! { #sig }
}

fn lower_signature_inputs(sig: &mut syn::Signature) {
    sig.inputs = core::mem::take(&mut sig.inputs)
        .into_iter()
        .flat_map(|input| lower_signature_input(&mut sig.generics, input))
        .collect();
}

fn lower_signature_input(generics: &mut syn::Generics, input: syn::FnArg) -> Vec<syn::FnArg> {
    let (pat, attrs, arg_ty) = match input {
        syn::FnArg::Receiver(receiver) => {
            let arg_ty = *receiver.ty;
            (parse_quote!(__co3_self), receiver.attrs, arg_ty)
        }
        syn::FnArg::Typed(arg) => {
            let syn::PatType { attrs, pat, ty, .. } = arg;
            (parse_quote!(#pat), attrs, *ty)
        }
    };

    if is_spread_arg(&attrs) {
        let arg_name = item_fn_input_ident(&pat);
        let (data_name, metadata_name) = spread_arg_names(arg_name);
        let spread_ty = item_fn_input_arg_type(&attrs, &arg_ty);
        let part1_ty = quote! { <#spread_ty as co3::slice::Spread2>::Part1 };
        let part2_ty = quote! { <#spread_ty as co3::slice::Spread2>::Part2 };

        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#part1_ty: co3::CFnArg));
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#part2_ty: co3::CFnArg));

        return vec![
            parse_quote!(#data_name: #part1_ty),
            parse_quote!(#metadata_name: #part2_ty),
        ];
    }

    let ffi_ty = item_fn_input_arg_type(&attrs, &arg_ty);

    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#ffi_ty: co3::CFnArg));

    vec![parse_quote!(#pat: #ffi_ty)]
}

fn lower_signature_output(sig: &mut syn::Signature) {
    let return_type = core::mem::replace(&mut sig.output, syn::ReturnType::Default);

    let return_type = match return_type {
        syn::ReturnType::Default => return,
        syn::ReturnType::Type(_, return_type) => *return_type,
    };

    let lowered_return_type = item_fn_output_type(&return_type);
    sig.output = parse_quote!(-> #lowered_return_type);

    sig.generics
        .make_where_clause()
        .predicates
        .push(parse_quote! {
            #lowered_return_type: co3::CFnReturn
        });
}

fn explicitize_signature_lifetimes(sig: &mut syn::Signature) {
    struct InputLifetimeCollector<'a> {
        out_lifetime: Option<syn::Lifetime>,
        generics: &'a mut syn::Generics,
        multiple_lifetimes: bool,
        is_self: bool,
        next_idx: usize,
    }

    impl<'a> InputLifetimeCollector<'a> {
        fn new(generics: &'a mut syn::Generics) -> Self {
            Self {
                generics,
                next_idx: 0,
                out_lifetime: None,
                is_self: false,
                multiple_lifetimes: false,
            }
        }

        fn push_lifetime_param(&mut self, lifetime: syn::Lifetime) {
            if self.generics.lt_token.is_none() {
                self.generics.lt_token = Some(Default::default());
                self.generics.gt_token = Some(Default::default());
            }

            self.generics
                .params
                .push(syn::GenericParam::Lifetime(syn::LifetimeParam::new(
                    lifetime,
                )));
        }

        fn explicitize_lifetime(&mut self, lifetime: &mut syn::Lifetime) -> syn::Lifetime {
            if lifetime.ident != "_" {
                return lifetime.clone();
            }

            let new_lifetime = format!("'__co3_{}", self.next_idx);
            let new_lifetime = syn::Lifetime::new(&new_lifetime, Span::call_site());

            self.push_lifetime_param(new_lifetime.clone());
            self.next_idx += 1;
            *lifetime = new_lifetime.clone();
            new_lifetime
        }

        fn record_lifetime<const IS_SELF: bool>(&mut self, lifetime: syn::Lifetime) {
            if self.out_lifetime.is_none() {
                self.out_lifetime = Some(lifetime);
                self.is_self = IS_SELF;
            } else {
                self.multiple_lifetimes = true;

                if IS_SELF {
                    self.out_lifetime = Some(lifetime);
                    self.is_self = true;
                } else if !self.is_self {
                    self.out_lifetime = None;
                }
            }
        }
    }

    impl VisitMut for InputLifetimeCollector<'_> {
        fn visit_receiver_mut(&mut self, node: &mut syn::Receiver) {
            if let Some((_, lifetime)) = &mut node.reference {
                let l = self.explicitize_lifetime(
                    lifetime.get_or_insert_with(|| syn::Lifetime::new("'_", Span::call_site())),
                );

                if let syn::Type::Reference(ty) = &mut *node.ty {
                    ty.lifetime = Some(l.clone());
                }

                self.record_lifetime::<true>(l);
            } else {
                self.visit_type_mut(&mut node.ty);
            }
        }

        fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
            let l = self.explicitize_lifetime(
                node.lifetime
                    .get_or_insert_with(|| syn::Lifetime::new("'_", Span::call_site())),
            );

            self.record_lifetime::<false>(l);
            self.visit_type_mut(&mut node.elem);
        }

        fn visit_lifetime_mut(&mut self, node: &mut syn::Lifetime) {
            let l = self.explicitize_lifetime(node);

            self.record_lifetime::<false>(l);
        }

        fn visit_type_bare_fn_mut(&mut self, _: &mut syn::TypeBareFn) {}
    }

    struct OutputLifetimeExplicator<'a> {
        lifetime: &'a syn::Lifetime,
    }

    impl VisitMut for OutputLifetimeExplicator<'_> {
        fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
            if node.lifetime.is_none() || matches!(&node.lifetime, Some(l) if l.ident == "_") {
                node.lifetime = Some(self.lifetime.clone());
            }

            self.visit_type_mut(&mut node.elem);
        }

        fn visit_lifetime_mut(&mut self, node: &mut syn::Lifetime) {
            if node.ident == "_" {
                *node = self.lifetime.clone();
            }
        }

        fn visit_type_bare_fn_mut(&mut self, _: &mut syn::TypeBareFn) {
            // Bare function pointer elision is scoped to the function pointer type.
        }
    }

    let mut input_collector = InputLifetimeCollector::new(&mut sig.generics);

    for input in &mut sig.inputs {
        input_collector.visit_fn_arg_mut(input);
    }

    let output_lifetime = (!input_collector.multiple_lifetimes || input_collector.is_self)
        .then_some(input_collector.out_lifetime)
        .flatten();

    if let syn::ReturnType::Type(_, output_ty) = &mut sig.output
        && let Some(lifetime) = &output_lifetime
    {
        OutputLifetimeExplicator { lifetime }.visit_type_mut(output_ty);
    }
}
fn synthesize_lifetime_bounds(sig: &mut syn::Signature) {
    #[derive(Default)]
    struct LifetimeUseCollector<'a> {
        bounds: BTreeMap<&'a syn::Lifetime, BTreeSet<&'a syn::Lifetime>>,
        parent_lifetimes: Vec<&'a syn::Lifetime>,
    }

    impl<'a> syn::visit::Visit<'a> for LifetimeUseCollector<'a> {
        fn visit_bare_fn_arg(&mut self, _: &'a syn::BareFnArg) {}

        fn visit_type_reference(&mut self, node: &'a syn::TypeReference) {
            if let Some(lifetime) = &node.lifetime {
                let parents = &self.parent_lifetimes;
                self.bounds.entry(lifetime).or_default().extend(parents);

                self.parent_lifetimes.push(lifetime);
                visit_type(self, &node.elem);
                self.parent_lifetimes.pop();
            } else {
                visit_type(self, &node.elem);
            }
        }

        fn visit_type_path(&mut self, node: &'a syn::TypePath) {
            if node.qself.is_some() {
                return;
            }

            syn::visit::visit_type_path(self, node);
        }

        fn visit_lifetime(&mut self, node: &'a syn::Lifetime) {
            let parents = &self.parent_lifetimes;
            self.bounds.entry(node).or_default().extend(parents);
        }
    }

    let mut lifetime_collector = LifetimeUseCollector::default();
    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => lifetime_collector.visit_type(&receiver.ty),
            syn::FnArg::Typed(arg) => lifetime_collector.visit_type(&arg.ty),
        }
    }
    if let syn::ReturnType::Type(_, ty) = &sig.output {
        lifetime_collector.visit_type(ty);
    }

    let bounds = lifetime_collector
        .bounds
        .into_iter()
        .map(|(k, v)| (k.clone(), v.into_iter().cloned().collect::<Vec<_>>()))
        .collect::<Vec<_>>();

    for (lhs, rhs) in bounds {
        if rhs.is_empty() {
            continue;
        }

        sig.generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#lhs: #(#rhs)+*));
    }
}

pub(crate) fn item_fn_input_arg_type(attrs: &[syn::Attribute], arg_ty: &Type) -> TokenStream {
    let c_type = quote! { <#arg_ty as co3::ExternC>::CType };

    match ownership_mode_for_arg(attrs) {
        OwnershipMode::ByValue => quote! { #c_type },
        OwnershipMode::Borrow => quote! {
            <#c_type as co3::borrow::BorrowCast>::AsConst
        },
    }
}

pub(crate) fn item_fn_output_type(return_ty: &Type) -> Type {
    parse_quote!(<#return_ty as co3::ExternC>::CType)
}

pub(crate) fn normalize_fn_signature(sig: &mut syn::Signature, self_ty: Option<&Type>) {
    for input in &mut sig.inputs {
        if let syn::FnArg::Receiver(syn::Receiver { attrs, ty, .. }) = input {
            *input = parse_quote! { #(#attrs)* __co3_self: #ty }
        }
    }

    if let Some(self_ty) = self_ty {
        SelfConcretizer { self_ty }.visit_signature_mut(sig);
    }
}

pub(crate) fn ownership_mode_for_arg(attrs: &[syn::Attribute]) -> OwnershipMode {
    if attrs.iter().any(is_by_val_attr) {
        return OwnershipMode::ByValue;
    }

    OwnershipMode::Borrow
}

pub(crate) struct SelfConcretizer<'a> {
    pub(crate) self_ty: &'a Type,
}

fn qualify_self_path(self_ty: &Type, rest: &Path) -> Type {
    if rest.segments.is_empty() {
        return self_ty.clone();
    }

    if matches!(self_ty, Type::Path(type_path) if type_path.qself.is_none()) {
        return parse_quote!(#self_ty::#rest);
    }

    parse_quote!(<#self_ty>::#rest)
}

impl VisitMut for SelfConcretizer<'_> {
    fn visit_type_mut(&mut self, node: &mut Type) {
        syn::visit_mut::visit_type_mut(self, node);

        if handle_id(node).is_some() {
            return;
        }

        let Type::Path(syn::TypePath { qself: None, path }) = node else {
            return;
        };
        let Some(first) = path.segments.first() else {
            return;
        };
        if first.ident != "Self" {
            return;
        }

        let mut rest = Path {
            leading_colon: None,
            segments: Default::default(),
        };

        rest.segments.extend(path.segments.iter().skip(1).cloned());
        *node = qualify_self_path(self.self_ty, &rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explicitized(mut sig: syn::Signature) -> syn::Signature {
        explicitize_signature_lifetimes(&mut sig);
        sig
    }

    fn with_synthesized_lifetime_bounds(mut sig: syn::Signature) -> syn::Signature {
        synthesize_lifetime_bounds(&mut sig);
        sig
    }

    #[test]
    fn explicitizes_single_elided_reference_input_and_output() {
        let sig = explicitized(parse_quote!(fn f(x: &u8) -> &u8));
        let expected: syn::Signature =
            parse_quote!(fn f<'__co3_0>(x: &'__co3_0 u8) -> &'__co3_0 u8);

        assert_eq!(sig, expected);
    }

    #[test]
    fn leaves_output_elided_for_multiple_input_lifetime_positions() {
        let sig = explicitized(parse_quote!(fn f(x: &u8, y: &u8) -> &u8));
        let expected: syn::Signature =
            parse_quote!(fn f<'__co3_0, '__co3_1>(x: &'__co3_0 u8, y: &'__co3_1 u8) -> &u8);

        assert_eq!(sig, expected);
    }

    #[test]
    fn assigns_receiver_lifetime_to_output_when_other_inputs_have_lifetimes() {
        let sig = explicitized(parse_quote!(fn f(&self, x: &u8) -> &u8));
        let expected: syn::Signature = parse_quote!(
            fn f<'__co3_0, '__co3_1>(&'__co3_0 self, x: &'__co3_1 u8) -> &'__co3_0 u8
        );

        assert_eq!(sig, expected);
    }

    #[test]
    fn counts_generic_lifetime_arguments_as_input_lifetime_positions() {
        let sig = explicitized(parse_quote!(fn f(x: Foo<'_>) -> Foo<'_>));
        let expected: syn::Signature =
            parse_quote!(fn f<'__co3_0>(x: Foo<'__co3_0>) -> Foo<'__co3_0>);

        assert_eq!(sig, expected);
    }

    #[test]
    fn nested_references_are_distinct_input_lifetime_positions() {
        let sig = explicitized(parse_quote!(fn f(x: &&u8) -> &u8));
        let expected: syn::Signature =
            parse_quote!(fn f<'__co3_0, '__co3_1>(x: &'__co3_0 &'__co3_1 u8) -> &u8);

        assert_eq!(sig, expected);
    }

    #[test]
    fn ignores_lifetimes_scoped_to_bare_function_pointer_inputs() {
        let sig = explicitized(parse_quote!(fn f(x: fn(&u8)) -> &u8));
        let expected: syn::Signature = parse_quote!(fn f(x: fn(&u8)) -> &u8);

        assert_eq!(sig, expected);
    }

    #[test]
    fn does_not_rewrite_bare_function_pointer_output_lifetimes() {
        let sig = explicitized(parse_quote!(fn f(x: &u8) -> fn(&u8) -> &u8));
        let expected: syn::Signature =
            parse_quote!(fn f<'__co3_0>(x: &'__co3_0 u8) -> fn(&u8) -> &u8);

        assert_eq!(sig, expected);
    }

    #[test]
    fn does_not_synthesize_bounds_from_bare_function_pointer_args() {
        let sig = with_synthesized_lifetime_bounds(parse_quote!(
            fn f<'a, 'b>(x: fn(&'a &'b u8))
        ));
        let expected: syn::Signature = parse_quote!(
            fn f<'a, 'b>(x: fn(&'a &'b u8))
        );

        assert_eq!(sig, expected);
    }

    #[test]
    fn does_not_synthesize_bounds_from_projections() {
        let sig = with_synthesized_lifetime_bounds(parse_quote!(
            fn f<'a, 'b>(
                x: <&'a <Foo as ExternC>::CType<'b> as BorrowCast>::AsConst
            )
        ));
        let expected: syn::Signature = parse_quote!(
            fn f<'a, 'b>(
                x: <&'a <Foo as ExternC>::CType<'b> as BorrowCast>::AsConst
            )
        );

        assert_eq!(sig, expected);
    }
}
