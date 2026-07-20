use std::collections::{BTreeMap, BTreeSet};

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Literal, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::{
    Attribute, GenericArgument, Type, TypePath, parse_quote, visit::Visit, visit_mut::VisitMut,
};

const MAX_TUPLE_ARITY: usize = 12;

pub(crate) fn co3_path() -> TokenStream {
    match crate_name("co3") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let name = format_ident!("{name}");
            quote!(::#name)
        }
        Err(_) => quote!(::co3),
    }
}

pub(crate) fn co3_alias() -> TokenStream {
    let co3 = co3_path();
    quote! {
        use #co3 as co3;
    }
}

pub(crate) fn push_error(errors: &mut Option<syn::Error>, err: syn::Error) {
    if let Some(errors) = errors {
        errors.combine(err);
    } else {
        *errors = Some(err);
    }
}

pub(crate) fn soft_for_arg(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("soft"))
}

pub(crate) fn is_type_erased(attr: &Attribute) -> bool {
    attr.path().is_ident("erased")
}

pub(crate) fn strip_internal_generic_param(param: &mut syn::TypeParam) {
    let is_erased = param.attrs.iter().any(is_type_erased);
    param.attrs.retain(|attr| !is_type_erased(attr));

    if is_erased {
        param.eq_token = None;
        param.default = None;
    }
}

pub(crate) fn erased_id_repr(param: &syn::TypeParam) -> Option<syn::Type> {
    let attr = param.attrs.iter().find(|attr| is_type_erased(attr))?;
    attr.parse_args().ok()
}

pub(crate) fn erased_abi_repr(param: &syn::TypeParam) -> syn::Type {
    param
        .default
        .clone()
        .unwrap_or_else(|| syn::parse_quote!(core::ffi::c_void))
}

pub(crate) struct ParamUseDetector<'a> {
    params: BTreeSet<&'a syn::Ident>,
    found: bool,
}

impl Visit<'_> for ParamUseDetector<'_> {
    fn visit_path(&mut self, node: &syn::Path) {
        if node.leading_colon.is_none()
            && let Some(first) = node.segments.first()
            && self.params.contains(&first.ident)
        {
            self.found = true;
            return;
        }

        syn::visit::visit_path(self, node);
    }
}

impl<'a> ParamUseDetector<'a> {
    pub fn new(params: impl IntoIterator<Item = &'a syn::Ident>) -> Self {
        Self {
            params: params.into_iter().collect(),
            found: false,
        }
    }

    pub fn type_mentions_param(&self, ty: &syn::Type) -> bool {
        let mut detector = Self::new(self.params.clone());
        detector.visit_type(ty);
        detector.found
    }

    pub fn predicate_mentions_param(&self, predicate: &syn::WherePredicate) -> bool {
        let mut detector = Self::new(self.params.clone());
        detector.visit_where_predicate(predicate);
        detector.found
    }
}

pub(crate) fn gen_store_name(arg_name: &syn::Ident) -> syn::Ident {
    format_ident!("__co3_{arg_name}_store")
}

fn calculate_tuple_depth(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    let mut depth = 1;
    let mut capacity = MAX_TUPLE_ARITY;
    while capacity < n {
        depth += 1;
        capacity *= MAX_TUPLE_ARITY;
    }
    depth
}

pub fn build_extern_c_type_tuple(types: &[&Type]) -> (TokenStream, TokenStream, Vec<TokenStream>) {
    if types.is_empty() {
        return (quote!(()), quote!(()), Vec::new());
    }

    let depth = calculate_tuple_depth(types.len());
    build_type_tuple_at_depth(types, depth)
}

fn build_type_tuple_at_depth(
    types: &[&Type],
    depth: usize,
) -> (TokenStream, TokenStream, Vec<TokenStream>) {
    if depth == 1 {
        let c_types = types.iter().map(|ty| quote!(<#ty as co3::ExternC>::CType));
        let accessors = (0..types.len())
            .map(|i| {
                let lit = Literal::usize_unsuffixed(i);
                quote!(#lit)
            })
            .collect();

        let c_tuple_ident = format_ident!("ReprCTuple{}", types.len());
        return (
            quote!((#(#types,)*)),
            quote!(co3::tuple::#c_tuple_ident<#(#c_types),*>),
            accessors,
        );
    }

    let chunk_size = MAX_TUPLE_ARITY.pow(depth as u32 - 1);
    let mut sub_tuples = Vec::new();
    let mut sub_c_tuples = Vec::new();
    let mut all_accessors = Vec::new();

    for (chunk_idx, chunk) in types.chunks(chunk_size).enumerate() {
        let (sub_tuple, sub_c_tuple, sub_accessors) = build_type_tuple_at_depth(chunk, depth - 1);
        sub_tuples.push(sub_tuple);
        sub_c_tuples.push(sub_c_tuple);

        let chunk_idx_lit = Literal::usize_unsuffixed(chunk_idx);
        for accessor in sub_accessors {
            all_accessors.push(quote!(#chunk_idx_lit.#accessor));
        }
    }

    let c_tuple_ident = format_ident!("ReprCTuple{}", sub_c_tuples.len());

    (
        quote!((#(#sub_tuples,)*)),
        quote!(co3::tuple::#c_tuple_ident<#(#sub_c_tuples),*>),
        all_accessors,
    )
}

pub(crate) struct DispatchMonomorphizer<'a> {
    subst: BTreeMap<&'a syn::Ident, &'a GenericArgument>,
}

impl<'a> DispatchMonomorphizer<'a> {
    pub(crate) fn new(
        impl_generics: &'a syn::Generics,
        entry: &'a syn::AngleBracketedGenericArguments,
    ) -> Self {
        let subst = impl_generics
            .params
            .iter()
            .filter(|param| !matches!(param, syn::GenericParam::Lifetime(_)))
            .zip(&entry.args)
            .filter_map(|(param, arg)| match param {
                syn::GenericParam::Type(param) => Some((&param.ident, arg)),
                syn::GenericParam::Const(param) => Some((&param.ident, arg)),
                syn::GenericParam::Lifetime(_) => None,
            })
            .collect();

        Self { subst }
    }
}

impl VisitMut for DispatchMonomorphizer<'_> {
    fn visit_item_impl_mut(&mut self, node: &mut syn::ItemImpl) {
        syn::visit_mut::visit_item_impl_mut(self, node);
    }

    fn visit_type_mut(&mut self, node: &mut Type) {
        syn::visit_mut::visit_type_mut(self, node);

        if let Type::Path(TypePath { qself: None, path }) = node
            && let Some(first) = path.segments.first()
            && let Some(subst) = self.subst.get(&first.ident).cloned()
        {
            if path.segments.len() == 1 {
                *node = parse_quote!(#subst);
                return;
            }

            let mut rest = syn::Path {
                leading_colon: None,
                segments: Default::default(),
            };

            for segment in path.segments.iter().skip(1) {
                rest.segments.push(segment.clone());
            }

            *node = parse_quote!(<#subst>::#rest);
        }
    }
}

pub(crate) fn is_drop_impl(impl_: &syn::ItemImpl) -> bool {
    impl_
        .trait_
        .as_ref()
        .is_some_and(|(_, path, _)| path.segments.last().is_some_and(|seg| seg.ident == "Drop"))
}

pub(crate) fn has_non_lifetime_generics(generics: &syn::Generics) -> bool {
    generics
        .params
        .iter()
        .any(|param| !matches!(param, syn::GenericParam::Lifetime(_)))
}

pub(crate) fn path_symbol_name(path: &syn::Path, generics: &syn::Generics) -> String {
    let mut builder = SymbolNameBuilder::new(generics);
    builder.visit_path(path);
    builder.finish()
}

pub(crate) fn type_symbol_name(ty: &Type, generics: &syn::Generics) -> String {
    let mut builder = SymbolNameBuilder::new(generics);
    builder.visit_type(ty);
    builder.finish()
}

#[derive(Default)]
struct SymbolNameBuilder {
    out: String,
    generic_params: BTreeMap<String, String>,
}

impl SymbolNameBuilder {
    fn new(generics: &syn::Generics) -> Self {
        let generic_params = generics
            .params
            .iter()
            .filter_map(|param| match param {
                syn::GenericParam::Type(param) => Some(param.ident.to_string()),
                _ => None,
            })
            .enumerate()
            .map(|(idx, ident)| (ident, format!("T{idx}")))
            .collect();

        Self {
            out: String::new(),
            generic_params,
        }
    }

    fn finish(self) -> String {
        sanitize_symbol_component(&self.out)
    }

    fn push_sep(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('_') {
            self.out.push('_');
        }
    }

    fn push_atom(&mut self, value: &str) {
        let sanitized = sanitize_symbol_component(value);

        if sanitized.is_empty() {
            return;
        }

        self.push_sep();
        self.out.push_str(&sanitized);
    }
}

impl Visit<'_> for SymbolNameBuilder {
    fn visit_path(&mut self, path: &syn::Path) {
        let Some(seg) = path.segments.last() else {
            self.push_atom("Self");
            return;
        };

        let ident = seg.ident.to_string();
        let atom = self.generic_params.get(&ident).cloned().unwrap_or(ident);
        self.push_atom(&atom);
        self.visit_path_arguments(&seg.arguments);
    }

    fn visit_path_arguments(&mut self, arguments: &syn::PathArguments) {
        if let syn::PathArguments::AngleBracketed(args) = arguments {
            for arg in &args.args {
                self.visit_generic_argument(arg);
            }
        }
    }

    fn visit_type_path(&mut self, type_path: &syn::TypePath) {
        self.visit_path(&type_path.path);
    }

    fn visit_type_reference(&mut self, reference: &syn::TypeReference) {
        self.push_atom(if reference.mutability.is_some() {
            "ref_mut"
        } else {
            "ref"
        });
        self.visit_type(&reference.elem);
    }

    fn visit_type_slice(&mut self, slice: &syn::TypeSlice) {
        self.push_atom("slice");
        self.visit_type(&slice.elem);
    }

    fn visit_type_array(&mut self, array: &syn::TypeArray) {
        self.push_atom("array");
        self.visit_type(&array.elem);
        self.push_atom(&array.len.to_token_stream().to_string());
    }

    fn visit_type_ptr(&mut self, ptr: &syn::TypePtr) {
        self.push_atom(if ptr.mutability.is_some() {
            "mut_ptr"
        } else {
            "const_ptr"
        });
        self.visit_type(&ptr.elem);
    }

    fn visit_type_tuple(&mut self, tuple: &syn::TypeTuple) {
        if tuple.elems.is_empty() {
            self.push_atom("unit");
        } else {
            self.push_atom("tuple");
            for elem in &tuple.elems {
                self.visit_type(elem);
            }
        }
    }

    fn visit_type_param_bound(&mut self, bound: &syn::TypeParamBound) {
        match bound {
            syn::TypeParamBound::Lifetime(_) => {}
            syn::TypeParamBound::Trait(trait_bound) => self.visit_path(&trait_bound.path),
            other => self.push_atom(&other.to_token_stream().to_string()),
        }
    }

    fn visit_expr(&mut self, expr: &syn::Expr) {
        self.push_atom(&expr.to_token_stream().to_string());
    }
}

fn sanitize_symbol_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_is_us = false;

    for ch in input.chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == '_';
        if keep {
            out.push(ch);
            prev_is_us = ch == '_';
        } else if !prev_is_us {
            out.push('_');
            prev_is_us = true;
        }
    }

    let out = out.trim_matches('_');
    if out.is_empty() {
        String::from("ty")
    } else {
        out.to_string()
    }
}
