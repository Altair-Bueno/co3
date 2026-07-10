//! Crate containing FFI related macro functionality
//!
//! # Example
//!
//! ```rust
//! #[cfg(feature = "export")]
//! use co3::export_C as ffi;
//! #[cfg(feature = "extern")]
//! use co3::extern_C as ffi;
//!
//! ffi! {
//!     #![symbol_prefix = "provider"]
//!
//!     type Local;
//!     fn make_local() -> Local;
//! }
//! ```
use std::{collections::BTreeMap, marker::PhantomData};

use manyhow::manyhow;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, ItemFn, ItemImpl, LitStr, Path, Result, Type, parse_quote, parse_quote_spanned,
    punctuated::Punctuated, spanned::Spanned, visit_mut::VisitMut,
};

use crate::{
    dispatch::{find_dispatch_attr, parse_dispatch_attr, parse_handle_id_attr},
    generate::{emit_decl_exports, expand_extern_import_decls},
    parse::ParsedForeignItem,
    repr::derive_repr_c,
    utils::{
        has_non_lifetime_generics, is_drop_impl, is_type_erased, path_symbol_name, push_error,
        strip_internal_generic_param, type_symbol_name,
    },
    validate::{validate_dispatch_self_id, validate_export_decls, validate_extern_decls},
};

mod dispatch;
mod ffi_fn;
mod generate;
mod parse;
mod repr;
mod utils;
mod validate;
mod wrapper;

enum ExportBlock {}
enum ExternBlock {}

trait InputKind {
    const IS_EXTERN: bool;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MacroFeatures {
    extern_types: bool,
    allocator_api: bool,
}

impl InputKind for ExportBlock {
    const IS_EXTERN: bool = false;
}

impl InputKind for ExternBlock {
    const IS_EXTERN: bool = true;
}

struct Input<T> {
    abi: syn::Abi,
    attrs: Vec<Attribute>,
    features: MacroFeatures,
    items: Vec<ForeignItem>,

    _kind: PhantomData<T>,
}

enum ForeignItem {
    Type(ForeignItemType),
    DynImpl(DynImpl),
    Impl(ItemImpl),
    Fn(ItemFn),
}

struct DynImpl {
    impl_: ItemImpl,
    args: Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
}

struct ForeignItemType {
    ty: syn::ForeignItemType,
    id: Option<Box<syn::Type>>,
    drop: Option<DropImpl>,

    dyn_self_impls: Vec<DynImpl>,
}

enum DropImpl {
    /// Impl dispatched on `dyn Self`
    DynSelfImpl(DynImpl),
    /// Any other dispatch
    DynImpl(DynImpl),
    /// Concrete impl
    Impl(ItemImpl),
}

/// Derive implementations of traits required to convert to and from an FFI-compatible type
///
/// # Attributes
///
/// * `#[reprC(NICHE_VALUE = <expr>)]` on a struct customizes [`co3::niche::Niche::NICHE_VALUE`]
/// * `#[reprC(is_valid = |[fieldN]| ...)]` on a struct or enum variant customizes validation
/// * `#[reprC(id($type))]` defines `co3::handle::HandleFamily::Kind`
///
/// ```
/// use co3::ReprC as ReprCAlias;
///
/// #[derive(ReprCAlias)]
/// pub struct Hello(u32);
/// ```
///
/// It assumes that the derive is imported and referred to by its original name.
#[manyhow]
#[proc_macro_derive(ReprC, attributes(reprC))]
pub fn repr_c_derive(item: syn::DeriveInput) -> Result<TokenStream> {
    if let Some(export_attr) = item.attrs.iter().find(|attr| {
        let last_seg = attr.path().segments.last();
        last_seg.is_some_and(|seg| seg.ident == "export")
    }) {
        let err_msg = "Opaque items can't derive ReprC";
        return Err(syn::Error::new_spanned(export_attr, err_msg));
    }

    derive_repr_c(&item)
}

#[manyhow]
#[proc_macro]
pub fn export_(input: TokenStream) -> Result<TokenStream> {
    export__(input)
}

#[manyhow]
#[proc_macro]
pub fn extern_(input: TokenStream) -> Result<TokenStream> {
    extern__(input)
}

/// [`export_`] with abi set to `"C"`
#[manyhow]
#[proc_macro]
#[expect(non_snake_case)]
pub fn export_C(input: TokenStream) -> Result<TokenStream> {
    export__(quote! {
        #![abi = "C"]
        #input
    })
}

/// [`extern_`] with abi set to `"C"`
#[manyhow]
#[proc_macro]
#[expect(non_snake_case)]
pub fn extern_C(input: TokenStream) -> Result<TokenStream> {
    extern__(quote! {
        #![abi = "C"]
        #input
    })
}

fn export__(input: TokenStream) -> Result<TokenStream> {
    let input = syn::parse2::<Input<ExportBlock>>(input)?;

    let Input {
        abi,
        features,
        items: decls,
        ..
    } = input;
    Ok(emit_decl_exports(abi, features, decls))
}

fn extern__(input: TokenStream) -> Result<TokenStream> {
    let input = syn::parse2::<Input<ExternBlock>>(input)?;

    let Input {
        abi,
        features,
        attrs,
        items,
        ..
    } = input;

    Ok(expand_extern_import_decls(abi, features, &attrs, items))
}

/// Quick and dirty way to define exports. Prefer using `export_C!` for production code
///
/// Works on `impl` blocks and free `fn` items.
/// Type items are not supported by this attribute; use `export_!`/`export_C!` for selecting individual exports.
///
/// # Example:
/// ```rust
/// use co3::{ReprC, export, export_C};
///
/// trait MyTrait {
///     fn foo();
/// }
///
/// #[derive(ReprC, Clone)]
/// #[repr(transparent)]
/// pub struct Foo(u8);
///
/// #[export("C")]
/// impl MyTrait for Foo {
///     fn foo() {}
/// }
///
/// #[export("C", symbol_prefix = "this_crate")]
/// impl Foo {
///     pub fn new(id: u8) -> Self {
///         Self(id)
///     }
///
///     pub fn new2(id: u8) -> Self {
///         Self(id)
///     }
/// }
///
/// #[export("C")]
/// fn selected_only() -> Foo {
///     Foo::new(7)
/// }
///
/// fn selected_only2() -> Foo {
///     Foo::new(7)
/// }
///
/// export_C! {
///     impl Foo {
///         pub fn new2(id: u8) -> Self;
///     }
///
///     fn selected_only2() -> Foo;
/// }
/// ```
#[manyhow]
#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    fn take_forwarded_export_fn_attrs(attrs: &mut Vec<Attribute>) -> Vec<Attribute> {
        let mut forwarded = Vec::new();

        attrs.retain(|attr| {
            if is_symbol_name_attr(attr) {
                forwarded.push(attr.clone());
                false
            } else {
                true
            }
        });

        forwarded
    }

    fn strip_fn_arg_attrs(signature: &mut syn::Signature) {
        for input in &mut signature.inputs {
            let attrs = match input {
                syn::FnArg::Receiver(node) => &mut node.attrs,
                syn::FnArg::Typed(node) => &mut node.attrs,
            };

            attrs.retain(|a| {
                !a.path().is_ident("by_val")
                    && !a.path().is_ident("soft")
                    && !a.path().is_ident("spread")
            });
        }
    }

    fn reject_lifetimes_attr(attrs: &[Attribute]) -> Result<()> {
        if let Some(attr) = attrs.iter().find(|attr| is_unsafe_lifetimes_attr(attr)) {
            return Err(syn::Error::new_spanned(
                attr,
                "`#[unsafe(lifetimes)]` is only supported inside `export_!`/`extern_!` macros",
            ));
        }

        Ok(())
    }

    let generics_err = "generic types are not supported by `#[export]`; use `export_!`/`export_C!`";
    let ExportAttrArgs { abi, symbol_prefix } = parse_export_attr(attr)?;

    let mut item = syn::parse2::<syn::Item>(item)?;
    let result = match &mut item {
        syn::Item::Struct(item) => {
            let item_id_ty = parse_handle_id_attr(&mut item.attrs)?.map(|ty| quote!(#[id(#ty)]));

            if has_non_lifetime_generics(&item.generics) {
                return Err(syn::Error::new_spanned(&item.generics, generics_err));
            }

            let vis = &item.vis;
            let ident = &item.ident;
            let generics = &item.generics;

            quote! { #item_id_ty #vis type #ident #generics; }
        }
        syn::Item::Enum(item) => {
            let item_id_ty = parse_handle_id_attr(&mut item.attrs)?.map(|ty| quote!(#[id(#ty)]));

            if has_non_lifetime_generics(&item.generics) {
                return Err(syn::Error::new_spanned(&item.generics, generics_err));
            }

            let vis = &item.vis;
            let ident = &item.ident;
            let generics = &item.generics;

            quote! { #item_id_ty #vis type #ident #generics; }
        }
        syn::Item::Union(item) => {
            let item_id = parse_handle_id_attr(&mut item.attrs)?.map(|ty| quote!(#[id(#ty)]));

            if has_non_lifetime_generics(&item.generics) {
                return Err(syn::Error::new_spanned(&item.generics, generics_err));
            }

            let vis = &item.vis;
            let ident = &item.ident;
            let generics = &item.generics;

            quote! { #item_id #vis type #ident #generics; }
        }
        syn::Item::Fn(item) => {
            reject_lifetimes_attr(&item.attrs)?;
            let attrs = take_forwarded_export_fn_attrs(&mut item.attrs);

            let vis = &item.vis;

            let mut sig = item.sig.clone();
            ensure_export_arg_names(&mut sig);
            strip_fn_arg_attrs(&mut item.sig);

            quote! { #(#attrs)* #vis #sig; }
        }
        syn::Item::Impl(impl_) => {
            let mut attrs = Vec::new();
            impl_.attrs.retain(|attr| {
                if attr.path().is_ident("dispatch") {
                    attrs.push(attr.clone());
                    false
                } else {
                    true
                }
            });

            let defaultness = &impl_.defaultness;
            let unsafety = &impl_.unsafety;
            let mut impl_generics = impl_.generics.clone();
            impl_generics.where_clause = None;
            let where_clause = &impl_.generics.where_clause;
            let trait_ = impl_.trait_.as_ref().map(|(_, path, _)| quote!(#path for));
            let self_ty = &impl_.self_ty;

            reject_lifetimes_attr(&impl_.attrs)?;
            let items = impl_
                .items
                .iter_mut()
                .filter_map(|item| {
                    let syn::ImplItem::Fn(method) = item else {
                        return None;
                    };

                    if let Err(err) = reject_lifetimes_attr(&method.attrs) {
                        return Some(Err(err));
                    }

                    let attrs = take_forwarded_export_fn_attrs(&mut method.attrs);
                    let (defaultness, vis) = (&method.defaultness, &method.vis);

                    let mut sig = method.sig.clone();
                    ensure_export_arg_names(&mut sig);
                    strip_fn_arg_attrs(&mut method.sig);

                    Some(Ok(quote! { #(#attrs)* #vis #defaultness #sig; }))
                })
                .collect::<Result<Vec<_>>>()?;

            let item_impl = quote! {
                #(#attrs)*
                #defaultness #unsafety impl #impl_generics #trait_ #self_ty #where_clause {
                    #(#items)*
                }
            };

            for param in &mut impl_.generics.params {
                let syn::GenericParam::Type(param) = param else {
                    continue;
                };

                strip_internal_generic_param(param);
            }

            item_impl
        }
        item => return Err(syn::Error::new_spanned(&*item, "Item not supported")),
    };

    let symbol_prefix_attr = symbol_prefix
        .as_ref()
        .map(|symbol_prefix| quote!(#![symbol_prefix = #symbol_prefix]));

    let exports = export__(quote! {
        #![abi = #abi]
        #symbol_prefix_attr
        #result
    })?;

    Ok(quote! {
        #item
        #exports
    })
}

impl<T: InputKind> Input<T> {
    fn new(
        mut attrs: Vec<Attribute>,
        symbol_prefix: LitStr,
        decls: Vec<ParsedForeignItem>,
    ) -> Result<Self> {
        let abi = parse_abi_attr(&mut attrs)?;
        let features = parse_feature_attrs(&mut attrs)?;

        for item in &decls {
            match item {
                ParsedForeignItem::Type(ForeignItemType {
                    ty,
                    id,
                    dyn_self_impls,
                    drop,
                }) => {
                    ensure_single_dispatch_attr(&ty.attrs)?;

                    for dispatch in dyn_self_impls {
                        ensure_single_dispatch_attr(&dispatch.impl_.attrs)?;
                    }

                    if let Some(drop) = drop {
                        let attrs = match drop {
                            DropImpl::DynSelfImpl(dispatch) => &dispatch.impl_.attrs,
                            DropImpl::DynImpl(dispatch) => &dispatch.impl_.attrs,
                            DropImpl::Impl(impl_) => &impl_.attrs,
                        };

                        ensure_single_dispatch_attr(attrs)?;
                    }

                    let has_non_lifetime_generics = ty
                        .generics
                        .params
                        .iter()
                        .any(|param| !matches!(param, syn::GenericParam::Lifetime(_)));

                    if has_non_lifetime_generics && id.is_none() {
                        let err_msg = "Generic types must declare handle #[id(...)]";
                        return Err(syn::Error::new_spanned(ty, err_msg));
                    }
                }
                ParsedForeignItem::Fn(ItemFn { attrs, .. })
                | ParsedForeignItem::Impl(ItemImpl { attrs, .. }) => {
                    ensure_single_dispatch_attr(attrs)?;
                }
            }
        }

        let decls = decls
            .into_iter()
            .map(|item| {
                Ok(match item {
                    ParsedForeignItem::Impl(mut impl_)
                        if find_dispatch_attr(&impl_.attrs).is_some() =>
                    {
                        let args = if T::IS_EXTERN && is_drop_impl(&impl_) {
                            Punctuated::<_, _>::default()
                        } else {
                            parse_dispatch_attr(&impl_)?
                        };

                        impl_.attrs.retain(|a| !a.path().is_ident("dispatch"));
                        strip_unsafe_lifetimes_attrs(&mut impl_.attrs);
                        for item in &mut impl_.items {
                            let syn::ImplItem::Fn(method) = item else {
                                continue;
                            };
                            strip_unsafe_lifetimes_attrs(&mut method.attrs);
                        }
                        ForeignItem::DynImpl(DynImpl { impl_, args })
                    }
                    ParsedForeignItem::Type(item) => ForeignItem::Type(item),
                    ParsedForeignItem::Impl(mut impl_) => {
                        strip_unsafe_lifetimes_attrs(&mut impl_.attrs);
                        for item in &mut impl_.items {
                            let syn::ImplItem::Fn(method) = item else {
                                continue;
                            };
                            strip_unsafe_lifetimes_attrs(&mut method.attrs);
                        }

                        ForeignItem::Impl(impl_)
                    }
                    ParsedForeignItem::Fn(mut item) => {
                        strip_unsafe_lifetimes_attrs(&mut item.attrs);
                        ForeignItem::Fn(item)
                    }
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let decls = pack_type_dispatch_impls::<T>(decls)?;
        let mut items = pack_type_drop_impls(decls)?;
        default_init(&symbol_prefix, &mut items)?;

        Ok(Self {
            abi,
            attrs,
            features,
            items,

            _kind: PhantomData,
        })
    }
}

fn default_init(symbol_prefix: &syn::LitStr, items: &mut [ForeignItem]) -> Result<()> {
    for item in items {
        match item {
            ForeignItem::DynImpl(_) => {}
            ForeignItem::Type(item) => {
                if let Some(drop) = &item.drop {
                    match drop {
                        DropImpl::DynSelfImpl(_) => {}
                        DropImpl::DynImpl(_) => {}
                        DropImpl::Impl(_) => {}
                    }

                    continue;
                }

                let impl_ = synthesize_default_drop_impl(symbol_prefix, &item.ty);
                item.drop = Some(DropImpl::Impl(impl_));
            }
            _ => {}
        }
    }

    Ok(())
}

impl syn::parse::Parse for Input<ExportBlock> {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut attrs = input.call(Attribute::parse_inner)?;
        let symbol_prefix =
            parse_symbol_prefix_attr(&mut attrs)?.unwrap_or_else(default_symbol_prefix);

        for attr in &attrs {
            if !attr.path().is_ident("abi") && !attr.path().is_ident("feature") {
                let err_msg = "Attribute not supported in this position";
                return Err(syn::Error::new_spanned(attr, err_msg));
            }
        }

        let mut decls = ExportBlock::parse_items(input)?;

        for item in &mut decls {
            match item {
                ParsedForeignItem::Fn(ItemFn { attrs, sig, .. }) => {
                    ensure_symbol_name_on_fn(attrs, &symbol_prefix, &sig.ident);
                }
                ParsedForeignItem::Impl(impl_) => {
                    let trait_ = impl_.trait_.as_ref().map(|(_, path, _)| path);
                    let self_ty = &impl_.self_ty;

                    for item in &mut impl_.items {
                        let syn::ImplItem::Fn(syn::ImplItemFn { attrs, sig, .. }) = item else {
                            continue;
                        };

                        ensure_symbol_name_on_impl_fn(
                            attrs,
                            &symbol_prefix,
                            trait_,
                            self_ty,
                            &impl_.generics,
                            &sig.ident,
                        );
                    }
                }
                _ => {}
            }
        }

        validate_export_decls(&decls)?;
        Self::new(attrs, symbol_prefix, decls)
    }
}

impl syn::parse::Parse for Input<ExternBlock> {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut attrs = input.call(Attribute::parse_inner)?;

        let symbol_prefix =
            parse_symbol_prefix_attr(&mut attrs)?.unwrap_or_else(default_symbol_prefix);
        let mut decls = ExternBlock::parse_items(input)?;

        for decl in &mut decls {
            match decl {
                ParsedForeignItem::Fn(ItemFn { attrs, sig, .. }) => {
                    let fn_name = &sig.ident;

                    if !attrs.iter().any(is_symbol_name_attr) {
                        let symbol_name = LitStr::new(
                            &format!("{}__{fn_name}", symbol_prefix.value()),
                            fn_name.span(),
                        );

                        attrs.push(parse_quote!(#[symbol_name = #symbol_name]));
                    }
                }
                ParsedForeignItem::Impl(impl_) => {
                    for item in &mut impl_.items {
                        let trait_ = impl_.trait_.as_ref().map(|(_, path, _)| path);
                        let self_ty = type_symbol_name(&impl_.self_ty, &impl_.generics);

                        let syn::ImplItem::Fn(syn::ImplItemFn { attrs, sig, .. }) = item else {
                            continue;
                        };

                        let fn_name = &sig.ident;
                        if !attrs.iter().any(is_symbol_name_attr) {
                            let symbol_name = if let Some(trait_) = trait_ {
                                let trait_ = path_symbol_name(trait_, &impl_.generics);
                                LitStr::new(
                                    &format!(
                                        "{}__{trait_}__{self_ty}__{fn_name}",
                                        symbol_prefix.value()
                                    ),
                                    fn_name.span(),
                                )
                            } else {
                                LitStr::new(
                                    &format!("{}__{self_ty}__{fn_name}", symbol_prefix.value()),
                                    fn_name.span(),
                                )
                            };

                            attrs.push(parse_quote!(#[symbol_name = #symbol_name]));
                        }
                    }
                }
                _ => {}
            }
        }

        validate_extern_decls(&decls)?;
        Self::new(attrs, symbol_prefix, decls)
    }
}

fn parse_abi_attr(attrs: &mut Vec<Attribute>) -> Result<syn::Abi> {
    let mut kept = Vec::with_capacity(attrs.len());

    let mut abi = None;
    for attr in attrs.drain(..) {
        if !attr.path().is_ident("abi") {
            kept.push(attr);
            continue;
        }

        let err_msg = "Expected `#![abi = \"...\"]`";
        let syn::Meta::NameValue(nv) = &attr.meta else {
            return Err(syn::Error::new_spanned(&attr, err_msg));
        };

        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(abi_lit),
            ..
        }) = &nv.value
        else {
            return Err(syn::Error::new_spanned(&nv.value, err_msg));
        };

        if abi.replace(syn::parse2(quote!(extern #abi_lit))?).is_some() {
            let msg = "Duplicate `#![abi = \"...\"]`";
            return Err(syn::Error::new_spanned(attr, msg));
        }
    }

    *attrs = kept;
    abi.ok_or(syn::Error::new(
        proc_macro2::Span::call_site(),
        "missing `#![abi = \"...\"]`",
    ))
}

fn default_symbol_prefix() -> LitStr {
    LitStr::new(
        &std::env::var("CARGO_CRATE_NAME").unwrap_or_else(|_| "co3".to_owned()),
        proc_macro2::Span::call_site(),
    )
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
                let err_msg = "Expected feature name in `#![feature(...)]`";
                return Err(syn::Error::new_spanned(meta, err_msg));
            };

            let Some(ident) = path.get_ident() else {
                let err_msg = "Expected feature name in `#![feature(...)]`";
                return Err(syn::Error::new_spanned(path, err_msg));
            };

            let feature = ident.to_string();
            let already_enabled = match feature.as_str() {
                "extern_types" => &mut features.extern_types,
                "allocator_api" => &mut features.allocator_api,
                _ => {
                    let err_msg = "Only `extern_types` and `allocator_api` are supported";
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

pub(crate) fn is_symbol_name_attr(attr: &Attribute) -> bool {
    attr.path().is_ident("symbol_name")
}

pub(crate) fn symbol_name_value(attr: &Attribute) -> Option<&syn::Expr> {
    if !is_symbol_name_attr(attr) {
        return None;
    }

    let syn::Meta::NameValue(nv) = &attr.meta else {
        return None;
    };

    Some(&nv.value)
}

struct ExportAttrArgs {
    abi: LitStr,
    symbol_prefix: Option<LitStr>,
}

impl syn::parse::Parse for ExportAttrArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let abi = input.parse::<LitStr>()?;
        let mut symbol_prefix = None;

        while !input.is_empty() {
            input.parse::<syn::Token![,]>()?;
            if input.peek(syn::Ident) {
                let ident = input.parse::<syn::Ident>()?;
                if ident != "symbol_prefix" {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "Expected `symbol_prefix = \"...\"`",
                    ));
                }

                input.parse::<syn::Token![=]>()?;
                let value = input.parse::<LitStr>()?;
                if symbol_prefix.replace(value).is_some() {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "duplicate `symbol_prefix = \"...\"`",
                    ));
                }
                continue;
            }

            return Err(input.error("Expected `symbol_prefix = \"...\"`"));
        }

        Ok(Self { abi, symbol_prefix })
    }
}

fn parse_export_attr(attr: TokenStream) -> Result<ExportAttrArgs> {
    if attr.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected ABI string literal, e.g. `#[export(\"C\")]`",
        ));
    }

    syn::parse2::<ExportAttrArgs>(attr)
}

pub(crate) fn is_unsafe_lifetimes_attr(attr: &Attribute) -> bool {
    if !attr.path().is_ident("unsafe") {
        return false;
    }

    let syn::Meta::List(meta_list) = &attr.meta else {
        return false;
    };

    let metas = meta_list
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .ok();
    let Some(metas) = metas else {
        return false;
    };

    metas
        .into_iter()
        .any(|meta| matches!(meta, syn::Meta::Path(path) if path.is_ident("lifetimes")))
}

pub(crate) fn strip_unsafe_lifetimes_attrs(attrs: &mut Vec<Attribute>) {
    let mut kept = Vec::with_capacity(attrs.len());

    for attr in attrs.drain(..) {
        if !attr.path().is_ident("unsafe") {
            kept.push(attr);
            continue;
        }

        let syn::Meta::List(meta_list) = &attr.meta else {
            kept.push(attr);
            continue;
        };

        let Ok(metas) = meta_list.parse_args_with(
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
        ) else {
            kept.push(attr);
            continue;
        };

        let kept_metas = metas
            .into_iter()
            .filter(|meta| !matches!(meta, syn::Meta::Path(path) if path.is_ident("lifetimes")))
            .collect::<syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>>();

        if kept_metas.is_empty() {
            continue;
        }

        kept.push(parse_quote!(#[unsafe(#kept_metas)]));
    }

    *attrs = kept;
}

fn pack_type_drop_impls(decls: Vec<ForeignItem>) -> Result<Vec<ForeignItem>> {
    fn insert_drop(
        explicit_drops: &mut BTreeMap<syn::Ident, DropImpl>,
        errors: &mut Option<syn::Error>,
        self_ty: syn::Ident,
        drop_impl: DropImpl,
    ) {
        if let Some(prev) = explicit_drops.insert(self_ty, drop_impl) {
            let impl_ = match prev {
                DropImpl::DynSelfImpl(d) | DropImpl::DynImpl(d) => d.impl_,
                DropImpl::Impl(impl_) => impl_,
            };

            let err_msg = "duplicate explicit `impl Drop` declaration";
            push_error(errors, syn::Error::new_spanned(impl_.self_ty, err_msg));
        }
    }

    fn self_ty_ident(impl_: &syn::ItemImpl) -> Option<syn::Ident> {
        let Type::Path(syn::TypePath { qself: None, path }) = &*impl_.self_ty else {
            return None;
        };

        if path.segments.len() > 1 {
            return None;
        }

        path.segments.last().map(|seg| seg.ident.clone())
    }

    const UNKNOWN_DROP: &str = "explicit `impl Drop` is only allowed for declared types";

    let mut kept_decls = Vec::with_capacity(decls.len());
    let mut explicit_drops = BTreeMap::new();
    let mut errors = None::<syn::Error>;

    for decl in decls {
        match decl {
            ForeignItem::Type(mut item) => {
                let mut kept_dyn_self_impls = Vec::with_capacity(item.dyn_self_impls.len());

                let self_ty = &item.ty.ident;
                for dyn_impl in item.dyn_self_impls {
                    if is_drop_impl(&dyn_impl.impl_) {
                        insert_drop(
                            &mut explicit_drops,
                            &mut errors,
                            self_ty.clone(),
                            DropImpl::DynSelfImpl(dyn_impl),
                        );
                    } else {
                        kept_dyn_self_impls.push(dyn_impl);
                    }
                }

                item.dyn_self_impls = kept_dyn_self_impls;
                kept_decls.push(ForeignItem::Type(item));
            }
            ForeignItem::Impl(impl_) => {
                if is_drop_impl(&impl_) {
                    if let Some(self_ty) = self_ty_ident(&impl_) {
                        insert_drop(
                            &mut explicit_drops,
                            &mut errors,
                            self_ty,
                            DropImpl::Impl(impl_),
                        );
                    } else {
                        let err = syn::Error::new_spanned(&impl_.self_ty, UNKNOWN_DROP);
                        push_error(&mut errors, err);
                    }
                } else {
                    kept_decls.push(ForeignItem::Impl(impl_));
                }
            }
            ForeignItem::DynImpl(dispatch) => {
                if is_drop_impl(&dispatch.impl_) {
                    if let Some(self_ty) = self_ty_ident(&dispatch.impl_) {
                        insert_drop(
                            &mut explicit_drops,
                            &mut errors,
                            self_ty,
                            DropImpl::DynImpl(dispatch),
                        );
                    } else {
                        let err = syn::Error::new_spanned(&dispatch.impl_.self_ty, UNKNOWN_DROP);
                        push_error(&mut errors, err);
                    }
                } else {
                    kept_decls.push(ForeignItem::DynImpl(dispatch));
                }
            }
            ForeignItem::Fn(item) => kept_decls.push(ForeignItem::Fn(item)),
        }
    }

    for decl in &mut kept_decls {
        let ForeignItem::Type(item) = decl else {
            continue;
        };

        item.drop = explicit_drops.remove(&item.ty.ident);
        if item.drop.is_none() && has_non_lifetime_generics(&item.ty.generics) {
            let err_msg = "generic types must provide explicit `impl Drop` declaration";
            push_error(&mut errors, syn::Error::new_spanned(&item.ty, err_msg));
        }
    }

    for drop_impl in explicit_drops.into_values() {
        let self_ty = match drop_impl {
            DropImpl::DynSelfImpl(dispatch) => dispatch.impl_.self_ty,
            DropImpl::DynImpl(dispatch) => dispatch.impl_.self_ty,
            DropImpl::Impl(impl_) => impl_.self_ty,
        };

        push_error(&mut errors, syn::Error::new_spanned(self_ty, UNKNOWN_DROP));
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(kept_decls)
}

pub(crate) fn trait_object_single_trait_bound(self_ty: &syn::Type) -> Option<&syn::TraitBound> {
    use syn::TraitBoundModifier;

    let syn::Type::TraitObject(trait_object) = self_ty else {
        return None;
    };

    let bound = trait_object.bounds.first()?;
    let syn::TypeParamBound::Trait(trait_bound) = bound else {
        return None;
    };
    if trait_bound.modifier != TraitBoundModifier::None || trait_bound.lifetimes.is_some() {
        return None;
    }

    Some(trait_bound)
}

fn pack_type_dispatch_impls<T: InputKind>(decls: Vec<ForeignItem>) -> Result<Vec<ForeignItem>> {
    fn is_self_only_dyn_dispatch(impl_: &ItemImpl) -> bool {
        trait_object_single_trait_bound(&impl_.self_ty).is_some()
            && !impl_
                .generics
                .type_params()
                .any(|param| param.attrs.iter().any(is_type_erased))
    }

    let mut type_dispatch = decls
        .iter()
        .filter_map(|decl| {
            if let ForeignItem::Type(item) = decl {
                Some((item.ty.ident.clone(), Vec::new()))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();

    let mut kept_decls = Vec::with_capacity(decls.len());
    let mut errors = None::<syn::Error>;

    for decl in decls {
        let ForeignItem::DynImpl(mut dispatch) = decl else {
            kept_decls.push(decl);
            continue;
        };

        let Some(syn::TraitBound { path, .. }) =
            trait_object_single_trait_bound(&dispatch.impl_.self_ty)
        else {
            kept_decls.push(ForeignItem::DynImpl(dispatch));
            continue;
        };
        if path.segments.len() > 1 {
            kept_decls.push(ForeignItem::DynImpl(dispatch));
            continue;
        }
        let Some(ident) = path.segments.first().map(|seg| seg.ident.clone()) else {
            kept_decls.push(ForeignItem::DynImpl(dispatch));
            continue;
        };
        if !type_dispatch.contains_key(&ident) {
            kept_decls.push(ForeignItem::DynImpl(dispatch));
            continue;
        }

        *dispatch.impl_.self_ty = parse_quote!(#path);
        normalize_self_handle_ids(&mut dispatch.impl_);

        if T::IS_EXTERN
            && let Err(err) = validate_dispatch_self_id(&dispatch.impl_)
        {
            push_error(&mut errors, err);
        }

        type_dispatch.entry(ident).or_default().push(dispatch);
    }

    for decl in &mut kept_decls {
        let ForeignItem::Type(item) = decl else {
            continue;
        };

        item.dyn_self_impls = type_dispatch.remove(&item.ty.ident).unwrap_or_default();
    }

    for decl in &kept_decls {
        let ForeignItem::DynImpl(dispatch) = decl else {
            continue;
        };

        if is_self_only_dyn_dispatch(&dispatch.impl_) {
            let err_msg = "`dyn Self` is only supported for declared types";
            let err = syn::Error::new_spanned(&dispatch.impl_.self_ty, err_msg);

            push_error(&mut errors, err);
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(kept_decls)
}

fn normalize_self_handle_ids(impl_: &mut syn::ItemImpl) {
    struct SelfHandleIdNormalizer {
        self_ty: syn::Type,
    }

    impl VisitMut for SelfHandleIdNormalizer {
        fn visit_type_mut(&mut self, node: &mut syn::Type) {
            syn::visit_mut::visit_type_mut(self, node);

            let self_ty = &self.self_ty;
            if *node == parse_quote!(<dyn #self_ty>::ID) {
                *node = parse_quote_spanned!(node.span()=> <dyn Self>::ID);
            }
        }
    }

    let mut normalizer = SelfHandleIdNormalizer {
        self_ty: (*impl_.self_ty).clone(),
    };

    normalizer.visit_item_impl_mut(impl_);
}

fn synthesize_default_drop_impl(symbol_prefix: &LitStr, ty: &syn::ForeignItemType) -> ItemImpl {
    let (impl_generics, ty_generics, where_clause) = ty.generics.split_for_impl();

    let ident = &ty.ident;
    let symbol_name = LitStr::new(
        &format!(
            "{}__{}__{}__drop",
            symbol_prefix.value(),
            path_symbol_name(&parse_quote!(Drop), &Default::default()),
            type_symbol_name(&parse_quote!(#ident #ty_generics), &ty.generics),
        ),
        ident.span(),
    );

    parse_quote! {
        impl #impl_generics Drop for #ident #ty_generics #where_clause {
            #[symbol_name = #symbol_name]
            fn drop(&mut self) {}
        }
    }
}

fn ensure_single_dispatch_attr(attrs: &[Attribute]) -> Result<()> {
    let mut dispatch_attrs = attrs.iter().filter(|attr| attr.path().is_ident("dispatch"));

    let mut errors = None::<syn::Error>;
    if dispatch_attrs.next().is_none() {
        return Ok(());
    };

    for attr in dispatch_attrs {
        let err = syn::Error::new_spanned(attr, "duplicate `#[dispatch]` attribute");

        if let Some(errors) = &mut errors {
            errors.combine(err);
        } else {
            errors = Some(err);
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn ensure_symbol_name_on_fn(
    attrs: &mut Vec<Attribute>,
    symbol_prefix: &LitStr,
    fn_name: &syn::Ident,
) {
    if !attrs.iter().any(is_symbol_name_attr) {
        let symbol_name = LitStr::new(
            &format!("{}__{fn_name}", symbol_prefix.value()),
            fn_name.span(),
        );

        attrs.push(parse_quote! {
            #[symbol_name = #symbol_name]
        });
    }
}

fn ensure_symbol_name_on_impl_fn(
    attrs: &mut Vec<Attribute>,
    symbol_prefix: &LitStr,
    trait_: Option<&Path>,
    self_ty: &Type,
    generics: &syn::Generics,
    fn_name: &syn::Ident,
) {
    if !attrs.iter().any(is_symbol_name_attr) {
        let default_symbol_name = trait_.map_or_else(
            || format!("{}__{}", type_symbol_name(self_ty, generics), fn_name),
            |trait_| {
                format!(
                    "{}__{}__{}",
                    path_symbol_name(trait_, generics),
                    type_symbol_name(self_ty, generics),
                    fn_name
                )
            },
        );
        let symbol_name = LitStr::new(
            &format!("{}__{default_symbol_name}", symbol_prefix.value()),
            proc_macro2::Span::call_site(),
        );

        attrs.push(parse_quote! {
            #[symbol_name = #symbol_name]
        });
    }
}

fn ensure_export_arg_names(sig: &mut syn::Signature) {
    let mut arg_idx = 1usize;

    for input in sig.inputs.iter_mut() {
        let syn::FnArg::Typed(syn::PatType { pat, .. }) = input else {
            continue;
        };

        let syn::Pat::Ident(ident) = &mut **pat else {
            let ident = format_ident!("arg{arg_idx}");
            *pat = parse_quote!(#ident);
            arg_idx += 1;
            continue;
        };

        ident.by_ref = None;
        ident.mutability = None;
        ident.subpat = None;
    }
}
