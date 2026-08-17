use std::collections::{BTreeMap, HashSet};

use proc_macro2::{Delimiter, Group, Ident, TokenStream, TokenTree};
use quote::quote;
use syn::{
    Attribute, Expr, FnArg, GenericArgument, GenericParam, ItemFn, ItemImpl, LitStr, PatType,
    Result, StaticMutability, Type, TypePath,
    parse::{ParseStream, Parser},
    parse_quote, parse_quote_spanned,
    punctuated::Punctuated,
    spanned::Spanned,
    visit::Visit,
    visit_mut::VisitMut,
};

use crate::{DeclKind, DispatchGroups, utils::push_error};

const FN_BODIES_NOT_ALLOWED_MSG: &str = "fn bodies are not allowed in declarations";
const ITEM_NOT_SUPPORTED_MSG: &str = "item not supported";
const EXPECTED_FEATURE_NAME_MSG: &str = "Expected feature name in `#![feature(...)]`";
const EXPECTED_HANDLE_ID_ATTR_MSG: &str = "expected `#[id(repr)]`";

pub(crate) enum ParsedForeignItem {
    Type(crate::ForeignItemType),
    Static(FfiStatic),
    Impl(ItemImpl),
    Fn(ItemFn),
}

pub(crate) struct FfiStatic {
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) vis: syn::Visibility,
    pub(crate) static_token: syn::Token![static],
    pub(crate) mutability: StaticMutability,
    pub(crate) ident: syn::Ident,
    pub(crate) ty: Box<Type>,
    pub(crate) expr: Option<Box<Expr>>,
}

pub(crate) struct FfiInput {
    pub(crate) kind: DeclKind,
    pub(crate) abi: syn::Abi,
    pub(crate) symbol_prefix: LitStr,
    pub(crate) features: MacroFeatures,
    pub(crate) failure_mode: FailureMode,
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) items: Vec<ParsedForeignItem>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MacroFeatures {
    pub(crate) extern_types: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FailureMode {
    #[default]
    Panic,
    Error,
}

struct FfiBody {
    attrs: Vec<Attribute>,
    items: Vec<ParsedForeignItem>,
}

struct ConstGenericArgNormalizer {
    const_params: HashSet<syn::Ident>,
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
    fn visit_lifetime(&mut self, lifetime: &syn::Lifetime) {
        if lifetime.ident != "_" {
            let err = "undeclared lifetime; consider using '_";
            self.push(syn::Error::new_spanned(lifetime, err));
        }
    }
}

impl VisitMut for ConstGenericArgNormalizer {
    fn visit_generic_argument_mut(&mut self, node: &mut GenericArgument) {
        syn::visit_mut::visit_generic_argument_mut(self, node);

        let GenericArgument::Type(Type::Path(TypePath { qself: None, path })) = node else {
            return;
        };
        let Some(ident) = path.get_ident() else {
            return;
        };
        if !self.const_params.contains(ident) {
            return;
        }

        *node = GenericArgument::Const(parse_quote!({#ident}));
    }
}

impl syn::parse::Parse for ParsedForeignItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let ahead = input.fork();
        let _ = ahead.call(syn::Attribute::parse_outer)?;
        if ahead.peek(syn::Token![impl]) {
            return Ok(Self::Impl(parse_impl_item(input)?));
        }

        let _ = ahead.parse::<syn::Visibility>()?;
        if ahead.peek(syn::Token![struct]) {
            return Err(ahead.error(ITEM_NOT_SUPPORTED_MSG));
        }
        if ahead.peek(syn::Token![type]) {
            let mut ty = if contains_dispatch_predicate(input)? {
                parse_type_item(input)?
            } else {
                input.parse::<syn::ForeignItemType>()?
            };
            let id = parse_handle_id_attr(&mut ty.attrs)?.map(Box::new);

            return Ok(Self::Type(crate::ForeignItemType {
                ty,
                id,
                self_impls: Vec::new(),
                drop: None,
            }));
        }
        if ahead.peek(syn::Token![static]) {
            return Ok(Self::Static(parse_static_item(input)?));
        }
        if is_fn_head(&ahead)? {
            return Ok(Self::Fn(parse_fn_item(input)?));
        }

        Err(input.error(ITEM_NOT_SUPPORTED_MSG))
    }
}

fn parse_static_item(input: ParseStream) -> Result<FfiStatic> {
    let attrs = input.call(syn::Attribute::parse_outer)?;
    let vis = input.parse()?;
    let static_token = input.parse()?;
    let mutability = input.parse()?;
    let ident = input.parse()?;
    input.parse::<syn::Token![:]>()?;
    let ty = input.parse()?;
    let expr = if input.peek(syn::Token![=]) {
        input.parse::<syn::Token![=]>()?;
        Some(input.parse()?)
    } else {
        None
    };
    input.parse::<syn::Token![;]>()?;

    Ok(FfiStatic {
        attrs,
        vis,
        static_token,
        mutability,
        ident,
        ty,
        expr,
    })
}

fn contains_dispatch_predicate(input: ParseStream) -> Result<bool> {
    let ahead = input.fork();
    let mut in_where_clause = false;

    while !ahead.is_empty() && !ahead.peek(syn::Token![;]) {
        let token = ahead.parse::<TokenTree>()?;
        match token {
            TokenTree::Ident(ident) if ident == "where" => in_where_clause = true,
            TokenTree::Punct(punct) if in_where_clause && punct.as_char() == '@' => {
                return Ok(true);
            }
            _ => {}
        }
    }

    Ok(false)
}

fn parse_items(input: ParseStream) -> Result<Vec<ParsedForeignItem>> {
    let mut items = Vec::new();

    while !input.is_empty() {
        items.push(input.parse()?);
    }

    Ok(items)
}

impl FfiInput {
    pub(crate) fn parse(tokens: TokenStream) -> Result<Self> {
        let FfiBody { mut attrs, items } = parse_ffi_body(tokens)?;
        let (kind, abi) = take_decl_attr(&mut attrs)?;
        let symbol_prefix =
            parse_symbol_prefix_attr(&mut attrs)?.unwrap_or_else(default_symbol_prefix);
        let failure_mode = parse_failure_attr(&mut attrs)?;
        let features = parse_feature_attrs(&mut attrs)?;

        Ok(Self {
            kind,
            abi,
            symbol_prefix,
            features,
            failure_mode,
            attrs,
            items,
        })
    }
}

fn default_symbol_prefix() -> LitStr {
    LitStr::new(
        &std::env::var("CARGO_CRATE_NAME").unwrap_or_else(|_| "co3".to_owned()),
        proc_macro2::Span::call_site(),
    )
}

fn parse_ffi_body(tokens: TokenStream) -> Result<FfiBody> {
    let parser = |input: syn::parse::ParseStream| -> Result<FfiBody> {
        let mut attr_tokens = TokenStream::new();

        while input.peek(syn::Token![#]) {
            let ahead = input.fork();
            ahead.parse::<syn::Token![#]>()?;
            if !ahead.peek(syn::Token![!]) {
                break;
            }

            input.parse::<syn::Token![#]>()?;
            input.parse::<syn::Token![!]>()?;
            let content;
            syn::bracketed!(content in input);
            let tokens = normalize_extern_attr_tokens(content.parse::<TokenStream>()?);
            attr_tokens.extend(quote!(#![#tokens]));
        }

        let attrs = Attribute::parse_inner.parse2(attr_tokens)?;
        let items = parse_items(input)?;

        Ok(FfiBody { attrs, items })
    };

    parser.parse2(tokens)
}

fn take_decl_attr(attrs: &mut Vec<Attribute>) -> Result<(DeclKind, syn::Abi)> {
    let mut decl = None;

    attrs.retain(|attr| {
        if !attr.path().is_ident("unsafe") {
            return true;
        }

        let next = parse_decl_attr(attr);
        if decl.replace(next).is_some() {
            decl = Some(Err(syn::Error::new_spanned(
                &attr.meta,
                "duplicate declaration kind attribute",
            )));
        }

        false
    });

    match decl {
        Some(result) => result,
        None => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "missing `#![unsafe(export(\"...\"))]` or `#![unsafe(extern(\"...\"))]`",
        )),
    }
}

fn parse_decl_attr(attr: &Attribute) -> Result<(DeclKind, syn::Abi)> {
    let err_msg = "expected `#![unsafe(export(\"...\"))]` or `#![unsafe(extern(\"...\"))]`";

    let syn::Meta::List(list) = &attr.meta else {
        return Err(syn::Error::new_spanned(attr, err_msg));
    };

    let nested = syn::parse2::<syn::Meta>(list.tokens.clone())
        .map_err(|_| syn::Error::new_spanned(attr, err_msg))?;
    let syn::Meta::List(nested) = nested else {
        return Err(syn::Error::new_spanned(attr, err_msg));
    };

    let path = &nested.path;
    let kind = if path.is_ident("export") {
        DeclKind::Export
    } else if path.is_ident("r#extern") {
        DeclKind::Extern
    } else {
        return Err(syn::Error::new_spanned(attr, err_msg));
    };

    let abi_lit = syn::parse2::<LitStr>(nested.tokens.clone())
        .map_err(|_| syn::Error::new_spanned(attr, err_msg))?;
    let abi = syn::parse2(quote!(extern #abi_lit))?;

    Ok((kind, abi))
}

fn parse_symbol_prefix_attr(attrs: &mut Vec<Attribute>) -> Result<Option<LitStr>> {
    let mut kept = Vec::with_capacity(attrs.len());

    let mut symbol_prefix = None;
    for attr in attrs.drain(..) {
        if !attr.path().is_ident("symbol_prefix") {
            kept.push(attr);
            continue;
        }

        let err_msg = "Expected `#![symbol_prefix = \"...\"]`";
        let syn::Meta::NameValue(nv) = &attr.meta else {
            return Err(syn::Error::new_spanned(&attr, err_msg));
        };

        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) = &nv.value
        else {
            return Err(syn::Error::new_spanned(&nv.value, err_msg));
        };

        if symbol_prefix.replace(value.clone()).is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "Duplicate `#![symbol_prefix = \"...\"]`",
            ));
        }
    }

    *attrs = kept;
    Ok(symbol_prefix)
}

fn parse_feature_attrs(attrs: &mut Vec<Attribute>) -> Result<MacroFeatures> {
    let mut kept = Vec::with_capacity(attrs.len());
    let mut features = MacroFeatures::default();

    for attr in attrs.drain(..) {
        if !attr.path().is_ident("feature") {
            kept.push(attr);
            continue;
        }

        let syn::Meta::List(list) = &attr.meta else {
            return Err(syn::Error::new_spanned(attr, "Expected `#![feature(...)]`"));
        };

        let metas =
            list.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)?;

        for meta in metas {
            let syn::Meta::Path(path) = &meta else {
                return Err(syn::Error::new_spanned(meta, EXPECTED_FEATURE_NAME_MSG));
            };

            let Some(ident) = path.get_ident() else {
                return Err(syn::Error::new_spanned(path, EXPECTED_FEATURE_NAME_MSG));
            };

            let feature = ident.to_string();
            let already_enabled = match feature.as_str() {
                "extern_types" => &mut features.extern_types,
                _ => {
                    let err_msg = "Only `extern_types` is supported";
                    return Err(syn::Error::new_spanned(ident, err_msg));
                }
            };

            if core::mem::replace(already_enabled, true) {
                return Err(syn::Error::new_spanned(
                    ident,
                    format!("Duplicate `{feature}` feature"),
                ));
            }
        }
    }

    *attrs = kept;
    Ok(features)
}

fn parse_failure_attr(attrs: &mut Vec<Attribute>) -> Result<FailureMode> {
    let mut kept = Vec::with_capacity(attrs.len());
    let mut failure_mode = None;

    for attr in attrs.drain(..) {
        if !attr.path().is_ident("failure") {
            kept.push(attr);
            continue;
        }

        let err_msg = "Expected `#![failure = \"panic\"]` or `#![failure = \"error\"]`";
        let syn::Meta::NameValue(nv) = &attr.meta else {
            return Err(syn::Error::new_spanned(&attr, err_msg));
        };

        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) = &nv.value
        else {
            return Err(syn::Error::new_spanned(&nv.value, err_msg));
        };

        let next = match value.value().as_str() {
            "panic" => FailureMode::Panic,
            "error" => FailureMode::Error,
            _ => return Err(syn::Error::new_spanned(value, err_msg)),
        };

        if failure_mode.replace(next).is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "Duplicate `#![failure = \"...\"]`",
            ));
        }
    }

    *attrs = kept;
    Ok(failure_mode.unwrap_or_default())
}

pub(crate) fn parse_dispatch_attr(
    attrs: &[syn::Attribute],
    generics: &syn::Generics,
) -> Result<DispatchGroups> {
    let Some(attr) = attrs.iter().find(|attr| attr.path().is_ident("erased")) else {
        return Ok(Default::default());
    };

    let syn::Meta::List(list) = &attr.meta else {
        let err = "tagged dispatch must provide concrete generic arguments";
        return Err(syn::Error::new_spanned(attr, err));
    };

    let groups = (|input: ParseStream| parse_dispatch_groups(generics, input))
        .parse2(list.tokens.clone())?;

    validate_dispatch_targets(generics, &groups)?;
    Ok(DispatchGroups { groups })
}

fn validate_dispatch_targets(
    generics: &syn::Generics,
    groups: &BTreeMap<Vec<syn::Ident>, Vec<syn::AngleBracketedGenericArguments>>,
) -> Result<()> {
    let mut lifetime_validator = LifetimeArgValidator { errors: None };

    let mut errors = None::<syn::Error>;
    for (group, targets) in groups {
        let err_msg = format!("expected {} concrete generic argument(s)", group.len(),);

        if targets.is_empty() {
            let err = "dispatch groups must contain at least one concrete target";
            push_error(&mut errors, syn::Error::new_spanned(&group[0], err));
            continue;
        }

        for target in targets {
            if target.args.len() != group.len() {
                let err = syn::Error::new_spanned(target, &err_msg);
                push_error(&mut errors, err);
                continue;
            }

            for (param, arg) in group.iter().zip(&target.args) {
                lifetime_validator.visit_generic_argument(arg);

                let matches_parameter_kind = generics.params.iter().any(|generic| match generic {
                    GenericParam::Type(generic) => {
                        generic.ident == *param && matches!(arg, GenericArgument::Type(_))
                    }
                    GenericParam::Const(generic) => {
                        generic.ident == *param && matches!(arg, GenericArgument::Const(_))
                    }
                    GenericParam::Lifetime(_) => false,
                });
                if !matches_parameter_kind {
                    let err = "argument kind must match declared parameter kind";
                    let err = syn::Error::new_spanned(arg, err);

                    push_error(&mut errors, err);
                }
            }
        }
    }

    if let Some(lifetime_errors) = lifetime_validator.errors {
        push_error(&mut errors, lifetime_errors);
    }

    errors.map_or(Ok(()), Err)
}

fn parse_dispatch_groups(
    generics: &syn::Generics,
    input: ParseStream,
) -> Result<BTreeMap<Vec<syn::Ident>, Vec<syn::AngleBracketedGenericArguments>>> {
    let mut assigned = HashSet::new();
    let mut groups = BTreeMap::new();

    let declared_types = generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Type(param) => Some(param.ident.clone()),
            GenericParam::Const(_) | GenericParam::Lifetime(_) => None,
        })
        .collect::<HashSet<_>>();

    let declared_consts = generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Const(param) => Some(param.ident.clone()),
            GenericParam::Type(_) | GenericParam::Lifetime(_) => None,
        })
        .collect::<HashSet<_>>();

    while !input.is_empty() {
        let dispatch_params = input.parse::<syn::PreciseCapture>()?;

        input.parse::<syn::Token![@]>()?;
        let targets;
        syn::parenthesized!(targets in input);
        let generic_args = Punctuated::<_, syn::Token![|]>::parse_terminated(&targets)?;

        let group = parse_dispatch_group(
            &declared_types,
            &declared_consts,
            &mut assigned,
            &dispatch_params,
        )?;
        groups.insert(group, generic_args.into_iter().collect());

        if input.is_empty() {
            break;
        }
        input.parse::<syn::Token![,]>()?;
    }

    Ok(groups)
}

fn parse_dispatch_group(
    declared_types: &HashSet<syn::Ident>,
    declared_consts: &HashSet<syn::Ident>,
    assigned: &mut HashSet<syn::Ident>,
    params: &syn::PreciseCapture,
) -> Result<Vec<syn::Ident>> {
    let mut group = Vec::with_capacity(params.params.len());

    if params.params.is_empty() {
        let err = "dispatch groups must contain at least one parameter";
        return Err(syn::Error::new_spanned(params, err));
    }

    for param in &params.params {
        let ident = match param {
            syn::CapturedParam::Ident(ident) => ident.clone(),
            syn::CapturedParam::Lifetime(lifetime) => {
                let err = "dispatch parameters cannot be lifetimes";
                return Err(syn::Error::new_spanned(lifetime, err));
            }
            _ => {
                let err = "dispatch parameters must be type or const parameters";
                return Err(syn::Error::new_spanned(param, err));
            }
        };

        if !declared_types.contains(&ident) && !declared_consts.contains(&ident) {
            let err = "dispatch parameter is not declared on this item";
            return Err(syn::Error::new_spanned(ident, err));
        }
        if !assigned.insert(ident.clone()) {
            let err = "dispatch parameter cannot appear in more than one group";
            return Err(syn::Error::new_spanned(ident, err));
        }

        group.push(ident);
    }

    Ok(group)
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
            return Err(syn::Error::new_spanned(attr, EXPECTED_HANDLE_ID_ATTR_MSG));
        };

        let ty = list
            .parse_args::<syn::Type>()
            .map_err(|_| syn::Error::new_spanned(&attr, EXPECTED_HANDLE_ID_ATTR_MSG))?;

        if id_ty.replace(ty).is_some() {
            return Err(syn::Error::new_spanned(attr, "duplicate `#[id(...)]`"));
        }
    }

    *attrs = kept;
    Ok(id_ty)
}

fn normalize_extern_attr_tokens(tokens: TokenStream) -> TokenStream {
    tokens
        .into_iter()
        .map(|token| match token {
            TokenTree::Ident(ident) if ident == "extern" => {
                let mut ident = Ident::new_raw("extern", ident.span());

                ident.set_span(ident.span());
                TokenTree::Ident(ident)
            }
            TokenTree::Group(group) => {
                let mut normalized = Group::new(
                    match group.delimiter() {
                        Delimiter::Parenthesis => Delimiter::Parenthesis,
                        Delimiter::Brace => Delimiter::Brace,
                        Delimiter::Bracket => Delimiter::Bracket,
                        Delimiter::None => Delimiter::None,
                    },
                    normalize_extern_attr_tokens(group.stream()),
                );
                normalized.set_span(group.span());
                TokenTree::Group(normalized)
            }
            token => token,
        })
        .collect()
}

fn is_fn_head(input: syn::parse::ParseStream) -> syn::Result<bool> {
    let ahead = input.fork();

    let _ = ahead.parse::<Option<syn::Token![const]>>()?;
    let _ = ahead.parse::<Option<syn::Token![async]>>()?;
    let _ = ahead.parse::<Option<syn::Token![unsafe]>>()?;
    let _ = ahead.parse::<Option<syn::Abi>>()?;
    let _ = ahead.parse::<Option<syn::Token![move]>>()?;

    Ok(ahead.peek(syn::Token![fn]))
}

fn preprocess_dispatch_where_clause(
    header: TokenStream,
) -> syn::Result<(TokenStream, Vec<Attribute>)> {
    fn split_top_level(tokens: TokenStream, separator: char) -> Vec<TokenStream> {
        let mut parts = Vec::new();
        let mut angle_depth = 0usize;
        for token in tokens {
            if parts.is_empty() {
                parts.push(TokenStream::new());
            }
            match &token {
                proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '<' => angle_depth += 1,
                proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '>' => {
                    angle_depth = angle_depth.saturating_sub(1)
                }
                proc_macro2::TokenTree::Punct(punct)
                    if punct.as_char() == separator && angle_depth == 0 =>
                {
                    parts.push(TokenStream::new());
                    continue;
                }
                _ => {}
            }
            parts.last_mut().unwrap().extend(core::iter::once(token));
        }
        parts
    }

    fn is_where(token: &proc_macro2::TokenTree) -> bool {
        matches!(token, proc_macro2::TokenTree::Ident(ident) if ident == "where")
    }

    fn normalize_targets(tokens: TokenStream) -> syn::Result<Vec<TokenStream>> {
        let tokens = tokens.into_iter().collect::<Vec<_>>();
        let target_tokens = if let [proc_macro2::TokenTree::Group(group)] = tokens.as_slice()
            && group.delimiter() == proc_macro2::Delimiter::Parenthesis
        {
            split_top_level(group.stream(), '|')
        } else {
            vec![tokens.into_iter().collect()]
        };

        for target in &target_tokens {
            syn::parse2::<syn::AngleBracketedGenericArguments>(target.clone()).map_err(|_| {
                let err = "expected a tagged-dispatch type list such as `<Type>`";
                syn::Error::new_spanned(target, err)
            })?;
        }

        Ok(target_tokens)
    }

    let tokens = header.into_iter().collect::<Vec<_>>();
    let Some(where_idx) = tokens.iter().position(is_where) else {
        return Ok((tokens.into_iter().collect(), Vec::new()));
    };

    let prefix = tokens[..where_idx].iter().cloned().collect::<TokenStream>();
    let predicates = tokens[where_idx + 1..]
        .iter()
        .cloned()
        .collect::<TokenStream>();

    let mut dispatch_span = None;
    let mut kept = Vec::new();

    let mut dispatch_predicates = Vec::new();
    for predicate in split_top_level(predicates, ',') {
        if predicate.is_empty() {
            continue;
        }

        let predicate_tokens = predicate.clone().into_iter().collect::<Vec<_>>();
        let Some(at_idx) = predicate_tokens.iter().position(
            |token| matches!(token, proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '@'),
        ) else {
            if !dispatch_predicates.is_empty() {
                let err = "tagged-dispatch predicates must be last in a `where` clause";
                return Err(syn::Error::new_spanned(predicate, err));
            }
            kept.push(predicate);
            continue;
        };

        let params = predicate_tokens[..at_idx]
            .iter()
            .cloned()
            .collect::<TokenStream>();
        let params = syn::parse2::<syn::PreciseCapture>(params).map_err(|_| {
            let err_msg = "expected dispatch parameters such as `use<T>` before `@`";
            syn::Error::new_spanned(&predicate, err_msg)
        })?;
        for param in &params.params {
            if let syn::CapturedParam::Lifetime(lifetime) = param {
                let err = "dispatch parameters cannot be lifetimes";
                return Err(syn::Error::new_spanned(lifetime, err));
            }
        }
        let targets = predicate_tokens[at_idx + 1..]
            .iter()
            .cloned()
            .collect::<TokenStream>();
        let targets = normalize_targets(targets)?;
        dispatch_span.get_or_insert(predicate.span());
        dispatch_predicates.push(quote!(#params @ (#(#targets)|*)));
    }

    let mut attrs = Vec::new();
    if !dispatch_predicates.is_empty() {
        let span = dispatch_span.expect("dispatch predicate has a span");
        attrs.push(parse_quote_spanned!(span=> #[erased(#(#dispatch_predicates),*)]));
    }

    if kept.is_empty() {
        return Ok((prefix, attrs));
    }
    Ok((quote!(#prefix where #(#kept),*), attrs))
}

fn preprocess_dispatch_params(header: TokenStream) -> syn::Result<TokenStream> {
    let mut out = TokenStream::new();
    let tokens = header.into_iter().collect::<Vec<_>>();
    let mut angle_depth = 0usize;
    let mut at_param_start = false;
    let mut generic_params_finished = false;

    let mut idx = 0usize;
    while idx < tokens.len() {
        let tt = tokens[idx].clone();

        if generic_params_finished {
            out.extend(std::iter::once(tt));
            idx += 1;
            continue;
        }

        match &tt {
            proc_macro2::TokenTree::Group(group)
                if angle_depth == 0 && group.delimiter() == proc_macro2::Delimiter::Parenthesis =>
            {
                generic_params_finished = true;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '<' => {
                angle_depth += 1;
                at_param_start = angle_depth == 1;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '>' => {
                angle_depth = angle_depth.saturating_sub(1);
                at_param_start = false;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == ',' && angle_depth == 1 => {
                at_param_start = true;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Ident(ident)
                if angle_depth == 1 && at_param_start && ident == "dyn" =>
            {
                let err_msg = "tagged-dispatch type parameters must use `dyn(TagTy) T`";

                let Some(proc_macro2::TokenTree::Group(group)) = tokens.get(idx + 1) else {
                    return Err(syn::Error::new(ident.span(), err_msg));
                };
                if group.delimiter() != proc_macro2::Delimiter::Parenthesis {
                    return Err(syn::Error::new(group.span(), err_msg));
                }

                let repr = syn::parse2::<Type>(group.stream()).map_err(|_| {
                    syn::Error::new(group.span(), "expected id repr in `dyn(TagTy) T`")
                })?;

                out.extend(quote!(#[erased(#repr)]));
                at_param_start = true;
                idx += 2;
            }
            proc_macro2::TokenTree::Punct(punct) if angle_depth == 1 && punct.as_char() == '#' => {
                at_param_start = false;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Ident(_) if angle_depth == 1 && at_param_start => {
                at_param_start = false;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            _ => {
                out.extend(std::iter::once(tt));
                idx += 1;
            }
        }
    }

    Ok(out)
}

struct PreprocessedArg {
    tokens: TokenStream,
}

fn push_by_val(attrs: &mut Vec<Attribute>) {
    attrs.push(parse_quote!(#[by_val]));
}

fn parse_move_by_val(
    input: syn::parse::ParseStream,
    attrs: &mut Vec<Attribute>,
) -> syn::Result<bool> {
    if input.peek(syn::Token![move]) {
        input.parse::<syn::Token![move]>()?;
        push_by_val(attrs);
        return Ok(true);
    }

    Ok(false)
}

impl PreprocessedArg {
    fn parse_with(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let mut merged_attrs = attrs;
        let _ = parse_move_by_val(input, &mut merged_attrs)?;

        let receiver = if input.peek(syn::Token![&]) {
            let ahead = input.fork();
            let _ = ahead.parse::<syn::Token![&]>()?;
            let _ = ahead.parse::<Option<syn::Lifetime>>()?;
            let _ = ahead.parse::<Option<syn::Token![mut]>>()?;
            ahead.peek(syn::Token![self])
        } else if input.peek(syn::Token![self]) {
            let ahead = input.fork();
            let _ = ahead.parse::<syn::Token![self]>()?;
            !ahead.peek(syn::Token![:])
        } else {
            false
        };

        if receiver {
            let mut receiver = input.parse::<syn::Receiver>()?;
            merged_attrs.append(&mut receiver.attrs);
            let ty = if let Some((and_token, lifetime)) = receiver.reference {
                let mutability = receiver.mutability;
                Type::Reference(syn::TypeReference {
                    and_token,
                    lifetime,
                    mutability,
                    elem: Box::new(parse_quote!(Self)),
                })
            } else {
                parse_quote!(Self)
            };

            return Ok(Self {
                tokens: quote!(#(#merged_attrs)* __co3_self: #ty),
            });
        }

        if input.peek(syn::Token![self]) {
            let ahead = input.fork();
            let _ = ahead.parse::<syn::Token![self]>()?;
            if ahead.peek(syn::Token![:]) {
                input.parse::<syn::Token![self]>()?;
                input.parse::<syn::Token![:]>()?;
                let ty = input.parse::<Type>()?;

                return Ok(Self {
                    tokens: quote! { #(#merged_attrs)* __co3_self: #ty },
                });
            }
        }

        let pat = syn::Pat::parse_single(input)?;
        let colon_token = input.parse::<syn::Token![:]>()?;
        let ty = input.parse::<Type>()?;
        let arg = PatType {
            attrs: merged_attrs,
            pat: Box::new(pat),
            colon_token,
            ty: Box::new(ty),
        };
        Ok(Self {
            tokens: quote!(#arg),
        })
    }
}

fn preprocess_signature_inputs(inputs: TokenStream) -> syn::Result<TokenStream> {
    let parser = move |input: syn::parse::ParseStream| -> syn::Result<TokenStream> {
        let mut args = Vec::new();
        while !input.is_empty() {
            args.push(PreprocessedArg::parse_with(input)?.tokens);
            if input.is_empty() {
                break;
            }
            input.parse::<syn::Token![,]>()?;
        }
        Ok(quote!(#(#args),*))
    };
    parser.parse2(inputs)
}

fn preprocess_signature_tokens(
    signature_tokens: TokenStream,
    attrs: &mut Vec<Attribute>,
) -> syn::Result<TokenStream> {
    let mut rewritten = Vec::new();
    let mut saw_inputs = false;
    let mut saw_fn = false;
    let mut by_val = false;
    let mut signature_tokens = signature_tokens.into_iter().peekable();
    while let Some(tt) = signature_tokens.next() {
        if !saw_fn && let proc_macro2::TokenTree::Ident(ident) = &tt {
            if ident == "move"
                && signature_tokens.peek().is_some_and(
                    |next| matches!(next, proc_macro2::TokenTree::Ident(next) if next == "fn"),
                )
            {
                if by_val {
                    return Err(syn::Error::new(ident.span(), "duplicate `move fn`"));
                }
                by_val = true;
                push_by_val(attrs);
                continue;
            }
            if ident == "fn" {
                saw_fn = true;
            }
        }
        if let proc_macro2::TokenTree::Group(group) = &tt
            && group.delimiter() == proc_macro2::Delimiter::Parenthesis
            && !saw_inputs
        {
            let rewritten_args = preprocess_signature_inputs(group.stream())?;
            let mut new_group =
                proc_macro2::Group::new(proc_macro2::Delimiter::Parenthesis, rewritten_args);
            new_group.set_span(group.span());
            rewritten.push(proc_macro2::TokenTree::Group(new_group));
            saw_inputs = true;
            continue;
        }
        rewritten.push(tt);
    }

    Ok(rewritten.into_iter().collect())
}

fn parse_signature(
    input: syn::parse::ParseStream,
    attrs: &mut Vec<Attribute>,
) -> syn::Result<syn::Signature> {
    let mut signature_tokens = TokenStream::new();
    while !input.peek(syn::Token![;]) && !input.peek(syn::token::Brace) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        signature_tokens.extend(std::iter::once(tt));
    }

    let (signature_tokens, dispatch_attrs) = preprocess_dispatch_where_clause(signature_tokens)?;
    attrs.extend(dispatch_attrs);
    let signature_tokens = preprocess_dispatch_params(signature_tokens)?;
    let rewritten = preprocess_signature_tokens(signature_tokens, attrs)?;
    let mut sig = syn::parse2::<syn::Signature>(rewritten)?;
    rewrite_erased_param_bounds_to_where_clause(&mut sig.generics);
    normalize_const_args_in_fn(&mut sig);

    Ok(sig)
}

fn parse_type_item(input: ParseStream) -> syn::Result<syn::ForeignItemType> {
    let mut attrs = input.call(syn::Attribute::parse_outer)?;
    let mut type_tokens = TokenStream::new();
    while !input.peek(syn::Token![;]) {
        let token = input.parse::<TokenTree>()?;
        type_tokens.extend(core::iter::once(token));
    }
    input.parse::<syn::Token![;]>()?;

    let (type_tokens, dispatch_attrs) = preprocess_dispatch_where_clause(type_tokens)?;
    attrs.extend(dispatch_attrs);
    syn::parse2(quote!(#(#attrs)* #type_tokens;))
}

fn parse_fn_item(input: syn::parse::ParseStream) -> syn::Result<ItemFn> {
    let mut attrs = input.call(syn::Attribute::parse_outer)?;
    let vis = input.parse::<syn::Visibility>()?;
    let sig = parse_signature(input, &mut attrs)?;
    if input.peek(syn::token::Brace) {
        return Err(input.error(FN_BODIES_NOT_ALLOWED_MSG));
    }
    input.parse::<syn::Token![;]>()?;
    syn::parse2(quote! {
        #(#attrs)* #vis #sig {}
    })
}

fn rewrite_erased_param_bounds_to_where_clause(generics: &mut syn::Generics) {
    let mut erased_bounds = Vec::<syn::WherePredicate>::new();

    for param in generics.type_params_mut() {
        let ident = &param.ident;

        if !param.attrs.iter().any(crate::utils::is_type_erased) {
            continue;
        }

        let bounds = core::mem::take(&mut param.bounds)
            .into_iter()
            .collect::<Vec<_>>();

        if !bounds.is_empty() {
            erased_bounds.push(parse_quote!(#ident: #(#bounds)+*));
        }
    }

    generics
        .make_where_clause()
        .predicates
        .extend(erased_bounds);
}

fn parse_impl_item(input: syn::parse::ParseStream) -> syn::Result<ItemImpl> {
    fn preprocess_impl_items(input: syn::parse::ParseStream) -> syn::Result<TokenStream> {
        let mut out = TokenStream::new();
        while !input.is_empty() {
            while input.peek(syn::Token![;]) {
                let _: syn::Token![;] = input.parse()?;
            }
            if input.is_empty() {
                break;
            }

            let mut attrs = input.call(syn::Attribute::parse_outer)?;
            let vis = input.parse::<syn::Visibility>()?;
            let ahead = input.fork();
            if ahead.peek(syn::Token![type]) {
                let item = input.parse::<syn::ImplItemType>()?;
                out.extend(quote!(#(#attrs)* #vis #item));
                continue;
            }
            if ahead.peek(syn::Token![const]) && !is_fn_head(&ahead)? {
                let item = input.parse::<syn::ImplItemConst>()?;
                out.extend(quote!(#(#attrs)* #vis #item));
                continue;
            }

            let sig = parse_signature(input, &mut attrs)?;
            let body = if input.peek(syn::token::Brace) {
                return Err(input.error(FN_BODIES_NOT_ALLOWED_MSG));
            } else {
                input.parse::<syn::Token![;]>()?;
                quote!({})
            };
            out.extend(quote!(#(#attrs)* #vis #sig #body));
        }
        Ok(out)
    }

    let mut attrs = input.call(syn::Attribute::parse_outer)?;
    let defaultness = input.parse::<Option<syn::Token![default]>>()?;
    let unsafety = input.parse::<Option<syn::Token![unsafe]>>()?;
    input.parse::<syn::Token![impl]>()?;

    let mut header = TokenStream::new();
    while !input.peek(syn::token::Brace) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        header.extend(std::iter::once(tt));
    }

    let content;
    syn::braced!(content in input);

    let items = preprocess_impl_items(&content)?;
    let (header, dispatch_attrs) = preprocess_dispatch_where_clause(header)?;
    attrs.extend(dispatch_attrs);
    let header = preprocess_dispatch_params(header)?;

    let impl_tokens = quote! {
        #(#attrs)* #defaultness #unsafety impl #header {
            #items
        }
    };

    let mut impl_: ItemImpl = syn::parse2(impl_tokens)?;
    rewrite_erased_param_bounds_to_where_clause(&mut impl_.generics);
    normalize_const_generic_args_in_impl(&mut impl_);

    normalize_self_handle_ids(&mut impl_);

    for item in &mut impl_.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };

        restore_synthetic_receiver(&mut method.sig);
        normalize_const_args_in_fn(&mut method.sig);
    }

    Ok(impl_)
}

fn normalize_self_handle_ids(impl_: &mut ItemImpl) {
    struct SelfHandleIdNormalizer {
        self_ty: syn::Type,
    }

    impl VisitMut for SelfHandleIdNormalizer {
        fn visit_type_mut(&mut self, node: &mut Type) {
            syn::visit_mut::visit_type_mut(self, node);

            let self_ty = &self.self_ty;
            if *node == parse_quote! { <dyn Self>::ID } {
                *node = if matches!(self_ty, Type::TraitObject(_)) {
                    parse_quote_spanned!(node.span()=> <#self_ty>::ID)
                } else {
                    parse_quote_spanned!(node.span()=> <dyn #self_ty>::ID)
                };
            }
        }
    }

    let mut normalizer = SelfHandleIdNormalizer {
        self_ty: (*impl_.self_ty).clone(),
    };

    normalizer.visit_item_impl_mut(impl_);
}

fn restore_synthetic_receiver(signature: &mut syn::Signature) {
    let Some(position) = signature.inputs.iter().position(|input| {
        matches!(
            input,
            FnArg::Typed(PatType { pat, .. })
                if matches!(pat.as_ref(), syn::Pat::Ident(pat_ident) if pat_ident.ident == "__co3_self")
        )
    }) else {
        return;
    };

    let mut inputs = core::mem::take(&mut signature.inputs)
        .into_iter()
        .collect::<Vec<_>>();

    let Some(FnArg::Typed(PatType { attrs, ty, .. })) = inputs.get_mut(position) else {
        signature.inputs = inputs.into_iter().collect();
        return;
    };

    let receiver_span = ty.span();
    let mut receiver: syn::Receiver = match &**ty {
        Type::Path(ty_path) if ty_path.qself.is_none() && ty_path.path.is_ident("Self") => {
            parse_quote_spanned!(receiver_span=> self)
        }
        Type::Reference(ty)
            if matches!(
                ty.elem.as_ref(),
                Type::Path(ty_path) if ty_path.qself.is_none() && ty_path.path.is_ident("Self")
            ) =>
        {
            let lifetime = &ty.lifetime;

            if ty.mutability.is_some() {
                parse_quote_spanned!(receiver_span=> &#lifetime mut self)
            } else {
                parse_quote_spanned!(receiver_span=> &#lifetime self)
            }
        }
        ty => {
            let mut receiver: syn::Receiver = parse_quote_spanned!(receiver_span=> self: #ty);
            receiver.colon_token = None;
            receiver
        }
    };

    receiver.attrs = core::mem::take(attrs);
    inputs[position] = FnArg::Receiver(receiver);
    signature.inputs = inputs.into_iter().collect();
}

fn normalize_const_generic_args_in_impl(impl_: &mut ItemImpl) {
    let const_params = impl_
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Const(param) => Some(param.ident.clone()),
            GenericParam::Lifetime(_) | GenericParam::Type(_) => None,
        })
        .collect();

    ConstGenericArgNormalizer { const_params }.visit_item_impl_mut(impl_);
}

fn normalize_const_args_in_fn(sig: &mut syn::Signature) {
    let const_params = sig
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Const(param) => Some(param.ident.clone()),
            GenericParam::Lifetime(_) | GenericParam::Type(_) => None,
        })
        .collect();

    ConstGenericArgNormalizer { const_params }.visit_signature_mut(sig);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_move_self_receiver() {
        let impl_ = "impl Value { fn name(move self); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::ImplItem::Fn(method) = &item.items[0] else {
            panic!("expected method");
        };
        let FnArg::Receiver(receiver) = &method.sig.inputs[0] else {
            panic!("expected receiver");
        };

        assert!(
            receiver
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("by_val"))
        );
    }

    #[test]
    fn parses_move_fn_return() {
        let item = Parser::parse_str(parse_fn_item, "move fn name() -> Value;").unwrap();

        assert!(item.attrs.iter().any(|attr| attr.path().is_ident("by_val")));
    }

    #[test]
    fn parses_qualified_move_fn_return() {
        let item = Parser::parse_str(
            parse_fn_item,
            "unsafe extern \"C\" move fn name() -> Value;",
        )
        .unwrap();

        assert!(item.attrs.iter().any(|attr| attr.path().is_ident("by_val")));
        assert!(item.sig.unsafety.is_some());
        assert!(item.sig.abi.is_some());
    }

    #[test]
    fn rejects_reordered_move_fn_return() {
        Parser::parse_str(parse_fn_item, "move unsafe fn name() -> Value;").unwrap_err();
        Parser::parse_str(parse_fn_item, "move async fn name() -> Value;").unwrap_err();
    }

    #[test]
    fn parses_dyn_dispatch_impl_param() {
        let impl_ = "impl<dyn(u8) T> Trait<T> for Value<T> { fn name(&mut self, value: &T); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::GenericParam::Type(param) = item.generics.params.first().unwrap() else {
            panic!("expected type param");
        };
        assert!(
            param
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("erased"))
        );
    }

    #[test]
    fn restores_receiver_after_id_arg() {
        let impl_ = "impl<dyn(u8) T> Trait<T> for Value { fn name(self_id: <dyn Self>::ID, &mut self, value: &T); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::ImplItem::Fn(method) = &item.items[0] else {
            panic!("expected method");
        };
        let FnArg::Receiver(receiver) = method.sig.inputs.iter().nth(1).unwrap() else {
            panic!("expected receiver");
        };

        assert!(receiver.reference.is_some());
        assert!(receiver.mutability.is_some());
    }

    #[test]
    fn normalizes_dyn_self_handle_id() {
        let impl_ = "impl<dyn(u32) U> Trait<T> for U { fn name(self_id: <dyn Self>::ID); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::ImplItem::Fn(method) = &item.items[0] else {
            panic!("expected method");
        };
        let FnArg::Typed(syn::PatType { ty, .. }) = &method.sig.inputs[0] else {
            panic!("expected typed arg");
        };

        assert_eq!(**ty, parse_quote! { <dyn U>::ID });
    }

    #[test]
    fn parses_dispatched_dyn_self_type_param() {
        let impl_ = "impl<T> Trait for dyn T { fn name(&self); }";
        Parser::parse_str(parse_impl_item, impl_).unwrap();
    }

    #[test]
    fn rejects_where_predicate_after_dispatch_predicate() {
        let err = preprocess_dispatch_where_clause(quote! {
            fn name<T>() where use<T> @ <u32>, T: Copy
        })
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("tagged-dispatch predicates must be last in a `where` clause")
        );
    }

    #[test]
    fn rejects_lifetime_dispatch_parameter() {
        let err = preprocess_dispatch_where_clause(quote! {
            fn name<'a, T>() where use<'a, T> @ <u32>
        })
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("dispatch parameters cannot be lifetimes")
        );
    }
}
