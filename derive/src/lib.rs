//! Crate containing FFI related macro functionality
//!
//! # Example
//!
//! ```rust
//! #![cfg(not(feature = "import"))]
//! struct Local(u8);
//!
//! #[cfg(not(feature = "import"))]
//! type LocalType = Box<Local>;
//! #[cfg(feature = "import")]
//! type LocalType = OwnedLocal;
//!
//! co3::ffi! {
//!     #![cfg_attr(not(feature = "import"), unsafe(export("C")))]
//!     #![cfg_attr(feature = "import", unsafe(extern("C")))]
//!
//!     #![symbol_prefix = "provider"]
//!
//!     type Local;
//!
//!     fn make_local() -> LocalType;
//! }
//! ```
use std::collections::{BTreeMap, HashMap};

use manyhow::manyhow;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, ItemFn, ItemImpl, LitStr, Path, Result, Type, parse_quote, parse_quote_spanned,
    spanned::Spanned, visit_mut::VisitMut,
};

use crate::{
    cfg_attr::{emit_macro_invocations, expand as expand_cfg_attr},
    dispatch::{find_dispatch_attr, synthesize_dispatch_handle_ids},
    generate::{expand_export_decls, expand_extern_decls},
    layout::derive_repr_c,
    parse::{FailureMode, FfiInput, MacroFeatures, ParsedForeignItem, parse_dispatch_attr},
    utils::{
        co3_path, has_non_lifetime_generics, is_drop_impl, is_type_erased, path_symbol_name,
        push_error, type_symbol_name,
    },
    validate::{validate_export_attrs, validate_export_decls, validate_extern_decls},
};

mod cfg_attr;
mod dispatch;
mod ffi_fn;
mod generate;
mod layout;
mod parse;
mod utils;
mod validate;
mod wrapper;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeclKind {
    Export,
    Extern,
}

struct Input {
    abi: syn::Abi,
    attrs: Vec<Attribute>,
    features: MacroFeatures,
    failure_mode: FailureMode,
    items: Vec<ForeignItem>,
}

enum ForeignItem {
    Type(ForeignItemType),
    Impl(Co3Impl),
    Fn(Co3Fn),
}

#[derive(Clone, Default)]
struct DispatchGroups {
    groups: BTreeMap<Vec<syn::Ident>, Vec<syn::AngleBracketedGenericArguments>>,
}

pub(crate) struct DispatchSelection<'a> {
    pub(crate) params: &'a [syn::Ident],
    pub(crate) target: &'a syn::AngleBracketedGenericArguments,
}

impl DispatchGroups {
    fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub(crate) fn groups(
        &self,
    ) -> impl Iterator<Item = (&[syn::Ident], &[syn::AngleBracketedGenericArguments])> {
        self.groups
            .iter()
            .map(|(params, targets)| (params.as_slice(), targets.as_slice()))
    }

    fn for_each_combination(&self, mut f: impl for<'a> FnMut(&[DispatchSelection<'a>])) {
        fn visit<'a>(
            groups: &[(&'a [syn::Ident], &'a [syn::AngleBracketedGenericArguments])],
            selections: &mut Vec<DispatchSelection<'a>>,
            f: &mut impl FnMut(&[DispatchSelection<'a>]),
        ) {
            let Some((params, targets)) = groups.first() else {
                f(selections);
                return;
            };

            for target in *targets {
                selections.push(DispatchSelection { params, target });
                visit(&groups[1..], selections, f);
                selections.pop();
            }
        }

        let groups = self.groups().collect::<Vec<_>>();
        visit(&groups, &mut Vec::new(), &mut f);
    }

    fn combined_with(&self, other: &Self) -> Self {
        let mut groups = self.groups.clone();
        groups.extend(other.groups.clone());
        Self { groups }
    }

    pub(crate) fn inject_unnamed_lifetimes(&mut self, generics: &mut syn::Generics) {
        for targets in self.groups.values_mut() {
            for target in targets {
                *target =
                    crate::dispatch::inject_unnamed_lifetimes(&mut generics.params, target.clone());
            }
        }
    }
}

struct Co3Impl {
    item: ItemImpl,
    dispatch_args: DispatchGroups,
    method_dispatch_args: HashMap<syn::Ident, DispatchGroups>,
}

impl Co3Impl {
    fn new(item: ItemImpl) -> Self {
        Self {
            item,
            dispatch_args: DispatchGroups::default(),
            method_dispatch_args: HashMap::default(),
        }
    }
}

impl core::ops::Deref for Co3Impl {
    type Target = ItemImpl;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl core::ops::DerefMut for Co3Impl {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

struct Co3Fn {
    item: ItemFn,
    dispatch_args: DispatchGroups,
}

impl core::ops::Deref for Co3Fn {
    type Target = ItemFn;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl core::ops::DerefMut for Co3Fn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

fn parse_dyn_methods(impl_: &mut ItemImpl) -> Result<HashMap<syn::Ident, DispatchGroups>> {
    let mut dyn_methods = HashMap::new();

    for item in &mut impl_.items {
        let syn::ImplItem::Fn(syn::ImplItemFn { attrs, sig, .. }) = item else {
            continue;
        };
        if find_dispatch_attr(attrs).is_none() {
            continue;
        }

        let args = parse_dispatch_attr(attrs, &sig.generics)?;
        attrs.retain(|attr| !attr.path().is_ident("erased"));

        if dyn_methods.insert(sig.ident.clone(), args).is_some() {
            return Err(syn::Error::new_spanned(&sig.ident, "duplicate method"));
        }

        synthesize_dispatch_handle_ids(Some(&impl_.self_ty), &sig.generics, &mut sig.inputs);
    }

    Ok(dyn_methods)
}

struct ForeignItemType {
    ty: syn::ForeignItemType,
    id: Option<Box<syn::Type>>,
    drop: Option<Co3Impl>,

    self_impls: Vec<Co3Impl>,
}

/// Generate a C-compatible counterpart and conversions to and from the Rust type.
///
/// A type deriving [`co3::ReprC`] can participate in [`ffi!`] declarations that use C ABI.
/// Note that most types will also require an implementation of [`rust_spec::RustSpec`].
///
/// # Helper Attributes
///
/// * `#[reprC(NICHE_VALUE = <expr>)]` on a struct customizes [`co3::niche::Niche::NICHE_VALUE`]
/// * `#[reprC(is_valid = |[fieldN]| ...)]` on a struct or enum variant customizes validation
/// * `#[reprC(id($type))]` defines [`co3::handle::HandleFamily::Kind`]
///
/// # Example
///
/// ```rust
/// use co3::ReprC
/// use rust_spec::RustSpec;
///
/// #[derive(RustSpec, ReprC)]
/// pub struct Hello(u32);
/// ```
#[manyhow]
#[proc_macro_derive(ReprC, attributes(reprC))]
pub fn repr_c_derive(item: syn::DeriveInput) -> Result<TokenStream> {
    derive_repr_c(&item)
}

/// Declare your FFI exports or imports.
///
/// `ffi!` generates the ABI-facing wrappers and the conversion glue through a familiar API. The
/// macro can be used to either produce the FFI bindings or to bind against an existing library.
/// It accepts free functions, inherent and trait `impl` blocks, and extern/opaque types. All
/// type conversions are checked for trap representations (with indirections followed) while
/// ownership transfer is made opt-in.
///
/// **The syntax of exports and imports is completely interchangeable.**
///
/// # Import from an external library
///
/// Use `#![unsafe(extern("ABI"))]` to declare the Rust-facing interface implemented by an external
/// library.
///
/// ## Safety
///
/// Import declarations must match the provider's contract for:
/// - ABI, symbol names, and function signatures
/// - ownership and lifetime requirements, when opted into
/// - pointer validity, mutability, and aliasing requirements
///
/// ```rust
/// use co3::{ffi, ReprC};
///
/// // `ReprC` generates a stable C-compatible companion even without an explicit `#[repr(...)]`.
/// // If given, an explicit representation would be leveraged to produce a more optimal mapping.
/// #[derive(Clone, Copy, ReprC)]
/// struct Value(u32);
///
/// trait Counter {
///     fn increment(&mut self, by: Value);
/// }
///
/// ffi! {
///     #![unsafe(extern("C"))]
///
///     type CounterHandle;
///
///     impl Counter for CounterHandle {
///         fn increment(&mut self, by: Value);
///     }
/// }
/// ```
///
/// # Export from Rust
///
/// Use `#![unsafe(export("ABI"))]` to expose existing Rust items. Although an ABI can be exported
/// on its own, the provider will often also provide a Rust client as well. In this common case,
/// export declarations are naturally paired with matching import declarations via a shared crate
/// that defines the application interface.
///
/// ## Safety
///
/// Export declarations must ensure:
/// - ABI symbol names do not collide with other symbols
/// - ownership and lifetime requirements, when opted into
/// - pointer-backed received input arguments are valid
///
/// ```rust
/// use co3::ffi;
///
/// #[cfg(not(feature = "import"))]
/// pub struct CounterHandle(u32);
///
/// #[cfg(not(feature = "import"))]
/// impl core::ops::AddAssign<u32> for CounterHandle {
///     fn add_assign(self, rhs: u32) {
///         self.0 += rhs;
///     }
/// }
///
/// #[cfg(not(feature = "import"))]
/// fn increment<T: core::ops::AddAssign<u32>>(value: &mut T) {
///     *value += 1;
/// }
///
/// ffi! {
///     #![cfg_attr(not(feature = "import"), unsafe(export("C")))]
///     #![cfg_attr(feature = "import", unsafe(extern("C")))]
///
///     type CounterHandle;
///
///     fn increment(value: &mut CounterHandle);
/// }
/// ```
///
/// # Naming convention
///
/// ABI symbols follow a stable naming convention:
///
/// - Free functions: `{prefix}__{function}`
/// - Inherent methods: `{prefix}__{SelfType}__{method}`
/// - Trait methods: `{prefix}__{TraitPath}__{SelfType}__{method}`
///
/// By default, `ffi!` uses `CARGO_CRATE_NAME` as the symbol prefix. `#![symbol_prefix = "..."]`
/// overrides the prefix for the entire scope, whereas `#[symbol_name = "..."]` overrides the name
/// for the declaration it is applied to:
///
/// ```rust
/// use co3::ffi;
///
/// trait Operations {
///     fn add(&self, left: u32, right: u32) -> u32;
/// }
///
/// ffi! {
///     #![unsafe(extern("C"))]
///     #![symbol_prefix = "my_lib"]
///
///     type Calculator;
///
///     // Linked as `my_lib__subtract`.
///     fn subtract(left: u32, right: u32) -> u32;
///
///     impl Calculator {
///         // Linked as `my_lib__Calculator__multiply`.
///         fn multiply(&self, left: u32, right: u32) -> u32;
///     }
///
///     impl Operations for Calculator {
///         // Linked as `library_add`.
///         #[symbol_name = "library_add"]
///         fn add(&self, left: u32, right: u32) -> u32;
///     }
/// }
/// ```
///
/// # Ownership transfer
///
/// In Rust, passing a value to a function transfers its ownership. Across an FFI boundary, however,
/// ownership transfer carries additional requirements which are a common cause of UB, such as
/// agreeing on the allocator and who is responsible for freeing the allocation.
///
/// Because it is designed to eliminate common footguns in FFI, `CO3` takes an opinionated stance here.
/// By default, all owned values (e.g. `Vec<T>`) are exported as references and immediately cloned on
/// the importing side. The universal guarantee is that any reference be valid for the duration of
/// the function call; past that, it is the user's responsibility to ensure reference validity.
///
/// Apply `move` to transfer ownership of an argument or return value:
///
/// ```rust
/// use co3::ffi;
///
/// fn passthrough(input: Vec<u8>) -> Vec<u8> {
///     input
/// }
///
/// fn clone_into(input: Vec<u8>) {
///     input
/// }
///
/// ffi! {
///     #![unsafe(export("C"))]
///
///     // `input` and the return both transfer ownership
///     move fn passthrough(move input: Vec<u8>) -> Vec<u8>;
///
///     // `input` is passed by reference
///     fn clone_into(input: Vec<u8>);
/// }
/// ```
///
/// # Tagged dispatch
///
/// Tagged dispatch is a dynamic dispatch over a closed set of concrete implementations commonly used
/// in C APIs. The concrete type is erased at the FFI boundary and carried as a shared representation
/// accompanied by a tag that identifies the concrete implementation to invoke. The tag position is
/// inferred at the start of the function parameter list but can also be specified explicitly. Every
/// tag-dispatched concrete instantiation is compile-time checked to have a C-compatible representation
/// with the same size and alignment of the declared shared ABI type.
///
/// In the following example:
/// - `T` is a tag-dispatched type parameter
/// - The tag type of parameter `T` is chosen as `u8`
/// - `u16` specifies the shared ABI representation used in the place of every concrete `T`.
/// - `where use<T> @ (...)` defines the set of concrete instantiations of dispatched types.
/// - `dyn Self` dispatches the `Self` parameter, but is only allowed for extern/opaque types.
///
/// The example corresponds to this C counterpart:
///
/// ```rust
/// use co3::{ffi, handle::Handle, rust_spec::RustSpec, ReprC};
///
/// #[derive(RustSpec, ReprC)]
/// #[reprC(id(u8))]
/// struct LocalCounter(u16);
///
/// trait Counter {
///     fn increment(&mut self, by: u8);
/// }
///
/// trait Reset {
///     fn reset(&mut self);
/// }
///
/// unsafe impl Handle for LocalCounter {
///     const ID: u8 = 1;
/// }
///
/// unsafe impl Handle for CounterHandle<i16> {
///     const ID: u8 = 3;
/// }
///
/// unsafe impl Handle for CounterHandle<u16> {
///     const ID: u8 = 4;
/// }
///
/// ffi! {
///     #![unsafe(extern("C"))]
///
///     #[id(u8)]
///     type CounterHandle<T>;
///
///     // Declare `T` as tag dispatched
///     impl<dyn(u8) T = u16> Counter for T
///     where
///         // Select the set of concrete instantiations of tyep parameter `T`
///         use<T> @ (<LocalCounter> | <CounterHandle<i16>> | <CounterHandle<u16>>)
///     {
///         // Make the tag-carrying argument position explicit.
///         fn increment(t_id: <dyn T>::ID, &mut self, by: u8);
///     }
///
///     // Dispatch the extern type itself
///     impl<T> Reset for dyn CounterHandle<T>
///     where
///         use<T> @ (<i16> | <u16>)
///     {
///         fn reset(self_id: <dyn Self>::ID, &mut self);
///     }
/// }
/// ```
///
/// # Soft references
///
/// Converting references to types with an unstable layout implies a clone of the pointed-to value.
/// The cloned value is lowered to it's C-compatible counterpart and a pointer to the store, rather
/// than the actual pointer, is returned. **The pointer identity is not preserved**.
///
/// Apply `#[soft]` to a function argument to opt into conversion of such types:
///
/// ```rust
/// use co3::ffi;
///
/// fn increment(value: (&(u8, u32), u32)) -> u8 {
///     value.0 + 1
/// }
///
/// ffi! {
///     #![unsafe(export("C"))]
///
///     fn increment(#[soft] value: (&(u8, u32), u32)) -> u8;
/// }
/// ```
///
/// # Spread operator
///
/// Rust slices are lowered into [`CSlice`](https://docs.rs/co3/latest/co3/slice/struct.CSlice.html)/[`CSliceMut`](https://docs.rs/co3/latest/co3/slice/struct.CSliceMut.html)
/// which are C-ABI containers holding a data pointer and a length. However, it is common for FFI APIs to instead accept those components as separate function arguments.
/// Prefix the argument type with `..` to export/import that form:
///
/// ```rust
/// # use co3::ffi;
///
/// ffi! {
///     #![unsafe(extern("system"))]
///
///     // imported as `sum(*const u32, usize)`
///     fn sum(values: ..&[u32]) -> u32;
/// }
/// ```
///
/// **This pattern is not limited to slices**; it applies to every type implementing the [`Spread2`](https://docs.rs/co3/latest/co3/slice/trait.Spread2.html) trait.
///
/// # Failure modes
///
/// Select how failures are reported over the FFI boundary:
/// - `#![failure = "error"]`: return type must implement [`co3::Error`].
/// - `#![failure = "panic"]`: panic on failure (the default).
///
/// ```rust
/// use co3::{ffi, Error, ReprC, rust_spec::RustSpec};
///
/// #[derive(RustSpec, ReprC)]
/// #[repr(u8)]
/// enum ApiError { InvalidRepresentation, UnknownHandle, SoftSync }
///
/// impl Error for ApiError {
///     fn trap_value() -> Self { Self::InvalidRepresentation }
///     fn unknown_handle() -> Self { Self::UnknownHandle }
///     fn soft_sync_error() -> Self { Self::SoftSync }
/// }
///
/// fn check_error(value: u8) -> ApiError {
///     let _ = value;
///     ApiError::InvalidRepresentation
/// }
///
/// ffi! {
///     #![unsafe(export("C"))]
///     #![failure = "error"]
///
///     fn check_error(value: u8) -> ApiError;
/// }
///
/// ffi! {
///     #![unsafe(extern("C"))]
///
///     fn check_panic(value: u8) -> u32;
/// }
/// ```
#[manyhow]
#[proc_macro]
pub fn ffi(input: TokenStream) -> Result<TokenStream> {
    let cfg_attr_variants = expand_cfg_attr(input.clone())?;

    if cfg_attr_variants.len() == 1 {
        let (kind, input) = Input::parse(input)?;

        let Input {
            abi,
            features,
            failure_mode,
            attrs,
            items,
            ..
        } = input;

        return Ok(match kind {
            DeclKind::Export => expand_export_decls(abi, features, failure_mode, items),
            DeclKind::Extern => expand_extern_decls(abi, features, failure_mode, &attrs, items),
        });
    }

    let co3 = co3_path();
    Ok(emit_macro_invocations(
        quote!(#co3::ffi),
        TokenStream::new(),
        cfg_attr_variants,
    ))
}

impl Input {
    fn parse(tokens: TokenStream) -> Result<(DeclKind, Self)> {
        let mut decls = Vec::new();

        let FfiInput {
            kind,
            abi,
            symbol_prefix,
            features,
            failure_mode,
            attrs,
            mut items,
        } = FfiInput::parse(tokens)?;

        match kind {
            DeclKind::Export => prepare_export_decls(&attrs, &symbol_prefix, &mut items),
            DeclKind::Extern => prepare_extern_decls(&symbol_prefix, &mut items),
        }?;

        for item in &items {
            match item {
                ParsedForeignItem::Type(ForeignItemType {
                    ty,
                    id,
                    self_impls,
                    drop,
                }) => {
                    ensure_single_dispatch_attr(&ty.attrs)?;

                    for dispatch in self_impls {
                        ensure_single_dispatch_attr(&dispatch.attrs)?;
                    }

                    if let Some(drop) = drop {
                        ensure_single_dispatch_attr(&drop.attrs)?;
                    }

                    if has_non_lifetime_generics(&ty.generics) && id.is_none() {
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

        for item in items {
            match item {
                ParsedForeignItem::Type(item) => decls.push(ForeignItem::Type(item)),
                ParsedForeignItem::Impl(mut impl_) => {
                    let dispatch_args = parse_dispatch_attr(&impl_.attrs, &impl_.generics)?;

                    if !dispatch_args.is_empty() {
                        let self_ty = &impl_.self_ty;

                        for item in &mut impl_.items {
                            let syn::ImplItem::Fn(syn::ImplItemFn { sig, .. }) = item else {
                                continue;
                            };

                            synthesize_dispatch_handle_ids(
                                Some(self_ty),
                                &impl_.generics,
                                &mut sig.inputs,
                            );
                        }

                        impl_.attrs.retain(|a| !a.path().is_ident("erased"));
                    }

                    strip_impl_explicit_lifetimes_attrs(&mut impl_);
                    let method_dispatch_args = parse_dyn_methods(&mut impl_)?;

                    decls.push(ForeignItem::Impl(Co3Impl {
                        item: impl_,
                        dispatch_args,
                        method_dispatch_args,
                    }));
                }
                ParsedForeignItem::Fn(mut item) => {
                    let dispatch_args = parse_dispatch_attr(&item.attrs, &item.sig.generics)?;

                    if !dispatch_args.is_empty() {
                        synthesize_dispatch_handle_ids(
                            None,
                            &item.sig.generics,
                            &mut item.sig.inputs,
                        );

                        item.attrs.retain(|a| !a.path().is_ident("erased"));
                    }

                    strip_explicit_lifetimes_attrs(&mut item.attrs);
                    decls.push(ForeignItem::Fn(Co3Fn {
                        item,
                        dispatch_args,
                    }));
                }
            }
        }

        let decls = pack_type_self_impls(decls)?;
        let mut items = pack_type_drop_impls(decls)?;

        if kind == DeclKind::Export {
            synthesize_default_drop_impls(&symbol_prefix, &mut items)?;
        }

        Ok((
            kind,
            Self {
                abi,
                attrs,
                features,
                failure_mode,
                items,
            },
        ))
    }
}

fn synthesize_default_drop_impls(
    symbol_prefix: &syn::LitStr,
    items: &mut [ForeignItem],
) -> Result<()> {
    for item in items {
        let ForeignItem::Type(item) = item else {
            continue;
        };
        if item.drop.is_some() {
            continue;
        }

        item.drop = Some(Co3Impl::new(synthesize_default_drop_impl(
            symbol_prefix,
            &item.ty,
        )));
    }

    Ok(())
}

fn prepare_export_decls(
    attrs: &[Attribute],
    symbol_prefix: &LitStr,
    decls: &mut [ParsedForeignItem],
) -> Result<()> {
    validate_export_attrs(attrs)?;

    for decl in decls.iter_mut() {
        ensure_symbol_names(decl, symbol_prefix);
    }

    validate_export_decls(decls)
}

fn prepare_extern_decls(symbol_prefix: &LitStr, decls: &mut [ParsedForeignItem]) -> Result<()> {
    for decl in decls.iter_mut() {
        ensure_symbol_names(decl, symbol_prefix);
    }

    validate_extern_decls(decls)
}

fn ensure_symbol_names(decl: &mut ParsedForeignItem, symbol_prefix: &LitStr) {
    match decl {
        ParsedForeignItem::Fn(ItemFn { attrs, sig, .. }) => {
            ensure_symbol_name_on_fn(attrs, symbol_prefix, &sig.ident);
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
                    symbol_prefix,
                    trait_,
                    self_ty,
                    &impl_.generics,
                    &sig.ident,
                );
            }
        }
        ParsedForeignItem::Type(_) => {}
    }
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

pub(crate) fn is_explicit_lifetimes_attr(attr: &Attribute) -> bool {
    attr.path().is_ident("explicit_lifetimes")
}

pub(crate) fn strip_explicit_lifetimes_attrs(attrs: &mut Vec<Attribute>) {
    attrs.retain(|attr| !is_explicit_lifetimes_attr(attr));
}

fn strip_impl_explicit_lifetimes_attrs(impl_: &mut ItemImpl) {
    strip_explicit_lifetimes_attrs(&mut impl_.attrs);

    for item in &mut impl_.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };

        strip_explicit_lifetimes_attrs(&mut method.attrs);
    }
}

fn pack_type_drop_impls(decls: Vec<ForeignItem>) -> Result<Vec<ForeignItem>> {
    fn insert_drop(
        explicit_drops: &mut BTreeMap<syn::Ident, Co3Impl>,
        errors: &mut Option<syn::Error>,
        self_ty: syn::Ident,
        drop_impl: Co3Impl,
    ) {
        if let Some(prev) = explicit_drops.insert(self_ty, drop_impl) {
            let err_msg = "duplicate explicit `impl Drop` declaration";
            push_error(errors, syn::Error::new_spanned(prev.item.self_ty, err_msg));
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
                let mut kept_self_impls = Vec::with_capacity(item.self_impls.len());

                let self_ty = &item.ty.ident;
                for dyn_impl in item.self_impls {
                    if is_drop_impl(&dyn_impl.item) {
                        insert_drop(&mut explicit_drops, &mut errors, self_ty.clone(), dyn_impl);
                    } else {
                        kept_self_impls.push(dyn_impl);
                    }
                }

                item.self_impls = kept_self_impls;
                kept_decls.push(ForeignItem::Type(item));
            }
            ForeignItem::Impl(impl_) => {
                if is_drop_impl(&impl_) {
                    if let Some(self_ty) = self_ty_ident(&impl_) {
                        insert_drop(&mut explicit_drops, &mut errors, self_ty, impl_);
                    } else {
                        let err = syn::Error::new_spanned(&impl_.self_ty, UNKNOWN_DROP);
                        push_error(&mut errors, err);
                    }
                } else {
                    kept_decls.push(ForeignItem::Impl(impl_));
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
        push_error(
            &mut errors,
            syn::Error::new_spanned(drop_impl.item.self_ty, UNKNOWN_DROP),
        );
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

fn pack_type_self_impls(decls: Vec<ForeignItem>) -> Result<Vec<ForeignItem>> {
    fn is_self_only_dyn_dispatch(impl_: &ItemImpl) -> bool {
        trait_object_single_trait_bound(&impl_.self_ty).is_some()
            && !impl_
                .generics
                .type_params()
                .any(|param| param.attrs.iter().any(is_type_erased))
    }

    fn declared_self_type(impl_: &ItemImpl) -> Option<syn::Ident> {
        match &*impl_.self_ty {
            Type::Path(syn::TypePath { qself: None, path }) if path.segments.len() == 1 => {
                path.segments.first().map(|segment| segment.ident.clone())
            }
            _ => trait_object_single_trait_bound(&impl_.self_ty)
                .filter(|bound| bound.path.segments.len() == 1)
                .and_then(|bound| bound.path.segments.first())
                .map(|segment| segment.ident.clone()),
        }
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
        let ForeignItem::Impl(dispatch) = decl else {
            kept_decls.push(decl);
            continue;
        };
        let Some(ident) = declared_self_type(&dispatch.item) else {
            kept_decls.push(ForeignItem::Impl(dispatch));
            continue;
        };
        if !type_dispatch.contains_key(&ident) {
            kept_decls.push(ForeignItem::Impl(dispatch));
            continue;
        }

        type_dispatch.entry(ident).or_default().push(dispatch);
    }

    for decl in &mut kept_decls {
        let ForeignItem::Type(item) = decl else {
            continue;
        };

        item.self_impls = type_dispatch.remove(&item.ty.ident).unwrap_or_default();
    }

    for decl in &kept_decls {
        let ForeignItem::Impl(dispatch) = decl else {
            continue;
        };

        if !dispatch.dispatch_args.is_empty() && is_self_only_dyn_dispatch(dispatch) {
            let err_msg = "`dyn Self` is only supported for declared types";
            let err = syn::Error::new_spanned(&dispatch.self_ty, err_msg);

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

pub(crate) fn materialize_dyn_self_dispatch(impl_: &mut syn::ItemImpl) -> bool {
    let Some(syn::TraitBound { path, .. }) = trait_object_single_trait_bound(&impl_.self_ty) else {
        return false;
    };

    *impl_.self_ty = parse_quote!(#path);
    normalize_self_handle_ids(impl_);
    true
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
    let mut dispatch_attrs = attrs.iter().filter(|attr| attr.path().is_ident("erased"));

    let mut errors = None::<syn::Error>;
    if dispatch_attrs.next().is_none() {
        return Ok(());
    };

    for attr in dispatch_attrs {
        let err = syn::Error::new_spanned(attr, "duplicate tagged-dispatch predicate");

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
