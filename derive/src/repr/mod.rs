use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Ident, spanned::Spanned as _, visit::Visit};

use crate::{
    generate::gen_handle_family_impl,
    repr::attr::{ReprKind, parse_repr},
    repr::item::{derive_fieldless_enum, derive_item},
    utils::push_error,
    validate::validate_niche_value_sized_tail,
};

mod attr;
mod borrow;
mod ctype;
mod item;
mod niche;
mod wide;

const FFI_TYPE_ATTR: &str = "reprC";

#[derive(Default)]
pub(super) struct ReprCAttrs {
    pub(super) niche_value: Option<syn::Expr>,
    pub(super) is_valid: Option<syn::ExprClosure>,
    pub(super) handle_id: Option<syn::Type>,
    pub(super) is_view: bool,
}

#[derive(Default)]
pub(super) struct VariantReprCAttrs {
    pub(super) is_valid: Option<syn::ExprClosure>,
}

fn parse_repr_c_attrs(attrs: &[Attribute]) -> syn::Result<ReprCAttrs> {
    let mut repr_c = ReprCAttrs::default();
    let mut found_attr = false;

    for attr in attrs
        .iter()
        .filter(|attr| attr.path().is_ident(FFI_TYPE_ATTR))
    {
        found_attr = true;

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("view") {
                if repr_c.is_view {
                    return Err(meta.error("Duplicate `view` within attribute"));
                }
                repr_c.is_view = true;
                return Ok(());
            }

            if meta.path.is_ident("id") {
                let content;
                syn::parenthesized!(content in meta.input);
                let value: syn::Type = content.parse()?;
                if repr_c.handle_id.replace(value).is_some() {
                    return Err(meta.error("Duplicate `id` within attribute"));
                }
                return Ok(());
            }

            if meta.path.is_ident("NICHE_VALUE") {
                let value: syn::Expr = meta.value()?.parse()?;
                if repr_c.niche_value.replace(value).is_some() {
                    return Err(meta.error("Duplicate `NICHE_VALUE` within attribute"));
                }
                return Ok(());
            }

            if meta.path.is_ident("is_valid") {
                let value: syn::ExprClosure = meta.value()?.parse()?;
                if repr_c.is_valid.replace(value).is_some() {
                    return Err(meta.error("Duplicate `is_valid` within attribute"));
                }
                return Ok(());
            }

            Err(meta.error("unknown type kind"))
        })?;
    }

    if !found_attr {
        return Ok(ReprCAttrs::default());
    }

    if repr_c.niche_value.is_none()
        && repr_c.is_valid.is_none()
        && repr_c.handle_id.is_none()
        && !repr_c.is_view
    {
        return Err(syn::Error::new_spanned(
            attrs
                .iter()
                .find(|attr| attr.path().is_ident(FFI_TYPE_ATTR))
                .expect("reprC attr was found"),
            "expected ffi type kind",
        ));
    }

    Ok(repr_c)
}

fn type_is_valid_closure(
    fields: &syn::Fields,
    is_valid: &mut Option<syn::ExprClosure>,
) -> syn::Result<()> {
    let Some(is_valid) = is_valid else {
        return Ok(());
    };

    if fields.len() != is_valid.inputs.len() {
        return Err(syn::Error::new_spanned(
            &is_valid.inputs,
            "`is_valid` closure must have exactly one argument per field",
        ));
    }

    for (input, field) in is_valid.inputs.iter_mut().zip(fields) {
        if matches!(input, syn::Pat::Type(_)) {
            continue;
        }

        let ty = &field.ty;
        let pat = input.clone();
        *input = syn::Pat::Type(syn::PatType {
            attrs: Vec::new(),
            pat: Box::new(pat),
            colon_token: Default::default(),
            ty: Box::new(syn::parse_quote!(&#ty)),
        });
    }

    Ok(())
}

pub(crate) fn derive_repr_c(input: &syn::DeriveInput) -> syn::Result<TokenStream> {
    let mut errors = None::<syn::Error>;

    let repr_attr = parse_repr(&input.attrs)?;
    let mut repr_c_attrs = parse_repr_c_attrs(&input.attrs)?;
    let mut variant_attrs = Vec::new();

    match &input.data {
        syn::Data::Struct(data) => {
            if matches!(data.fields, syn::Fields::Unit) {
                let err_msg = "Unit structs are not supported yet";
                push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));
            }

            validate_fields_no_ffi_type_attr(&data.fields, &mut errors);
            if repr_c_attrs.niche_value.is_some()
                && let Err(err) = validate_niche_value_sized_tail(&data.fields)
            {
                push_error(&mut errors, err);
            }
            if let Err(err) = type_is_valid_closure(&data.fields, &mut repr_c_attrs.is_valid) {
                push_error(&mut errors, err);
            }
        }
        syn::Data::Enum(data) => {
            if repr_c_attrs.is_valid.is_some() {
                let err_msg = "`is_valid` is only supported on structs or enum variants";
                push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));
            }

            if repr_c_attrs.niche_value.is_some() {
                let err_msg = "`NICHE_VALUE` is only supported on structs";
                push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));
            }

            if matches!(repr_attr.as_ref(), Some(ReprKind::C(None))) {
                let err_msg = "#[repr(C)]` not supported; use `#[repr(int)]`/`#[repr(C, int)]`";
                push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));
            }

            for variant in &data.variants {
                validate_fields_no_ffi_type_attr(&variant.fields, &mut errors);
                if variant.discriminant.is_some() {
                    let err_msg = "Explicit discriminants are not supported";
                    push_error(&mut errors, syn::Error::new(variant.span(), err_msg));
                }

                let mut variant_repr_c_attrs = parse_repr_c_attrs(&variant.attrs)?;
                if let Err(err) =
                    type_is_valid_closure(&variant.fields, &mut variant_repr_c_attrs.is_valid)
                {
                    push_error(&mut errors, err);
                }
                if variant_repr_c_attrs.niche_value.is_some() {
                    let err_msg = "`NICHE_VALUE` is only supported on types";
                    push_error(&mut errors, syn::Error::new(variant.span(), err_msg));
                }
                if variant_repr_c_attrs.handle_id.is_some() {
                    let err_msg = "`id` is only supported on types";
                    push_error(&mut errors, syn::Error::new(variant.span(), err_msg));
                }
                if variant_repr_c_attrs.is_view {
                    let err_msg = "`view` is only supported on types";
                    push_error(&mut errors, syn::Error::new(variant.span(), err_msg));
                }
                variant_attrs.push(VariantReprCAttrs {
                    is_valid: variant_repr_c_attrs.is_valid,
                });
            }
        }
        syn::Data::Union(_) => {
            return Err(syn::Error::new_spanned(input, "Unions are not supported"));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    let mut generics = input.generics.clone();
    generics.make_where_clause();
    let tokens = match &input.data {
        syn::Data::Struct(_) => derive_item(repr_attr.as_ref(), input, &repr_c_attrs, &[]),
        syn::Data::Enum(data) if data.variants.is_empty() => {
            // TODO: Support uninhabited enums. yes, it is possible
            let err_msg = "Uninhabited enum is a never type. You can declare it as an opaque type in `ffi!` with `type Foo;`";
            push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));

            quote! {}
        }
        syn::Data::Enum(data) => {
            if data
                .variants
                .iter()
                .all(|v| matches!(v.fields, syn::Fields::Unit))
            {
                derive_fieldless_enum(repr_attr.as_ref(), &input.ident, &generics, &data.variants)
            } else {
                derive_item(repr_attr.as_ref(), input, &repr_c_attrs, &variant_attrs)
            }
        }
        syn::Data::Union(_) => unreachable!(),
    };

    if let Some(errors) = errors {
        Err(errors)
    } else {
        let drop_impl_assert = assert_no_drop(&generics, &input.ident);

        let handle_family_impl = repr_c_attrs
            .handle_id
            .as_ref()
            .map(|id| gen_handle_family_impl(&input.ident, &generics, id));

        Ok(quote! {
            #handle_family_impl
            #drop_impl_assert

            #tokens
        })
    }
}

/// Finds an optional single attribute with specified name.
///
/// Returns `None` if no attributes with specified name are found.
///
/// Emits an error into accumulator if multiple attributes with specified name are found.
pub fn find_single_attr_opt<'a>(
    attr_name: &str,
    attrs: &'a [syn::Attribute],
) -> syn::Result<Option<&'a syn::Attribute>> {
    fn join_spans(spans: impl IntoIterator<Item = proc_macro2::Span>) -> Option<proc_macro2::Span> {
        let mut iter = spans.into_iter();
        let first = iter.next()?;
        Some(iter.try_fold(first, |a, b| a.join(b)).unwrap_or(first))
    }

    let matching_attrs = attrs
        .iter()
        .filter(|a| a.path().is_ident(attr_name))
        .collect::<Vec<_>>();
    let attr = match *matching_attrs.as_slice() {
        [] => return Ok(None),
        [attr] => attr,
        [attr, ref tail @ ..] => {
            return Err(syn::Error::new(
                join_spans(tail.iter().map(syn::spanned::Spanned::span))
                    .unwrap_or_else(|| attr.span()),
                format!("Only one #[{}] attribute is allowed!", attr_name),
            ));
        }
    };

    Ok(Some(attr))
}

fn validate_fields_no_ffi_type_attr(fields: &syn::Fields, errors: &mut Option<syn::Error>) {
    for field in fields {
        match find_single_attr_opt(FFI_TYPE_ATTR, &field.attrs) {
            Ok(Some(attr)) => {
                let err_msg = "`is_valid` is only supported on structs or enum variants";
                push_error(errors, syn::Error::new_spanned(attr, err_msg));
            }
            Ok(None) => {}
            Err(err) => push_error(errors, err),
        }
    }
}

/// Check if a type contains any of the type parameters from generics
fn is_type_parameterized(ty: &syn::Type, generics: &syn::Generics) -> bool {
    /// Visitor to check if a type contains any of the specified type parameters
    struct TypeParamVisitor<'a> {
        type_params: &'a [&'a syn::Ident],
        is_generic: bool,
    }

    impl Visit<'_> for TypeParamVisitor<'_> {
        fn visit_type_path(&mut self, type_path: &syn::TypePath) {
            if type_path.qself.is_none()
                && let Some(first_segment) = type_path.path.segments.first()
                && self.type_params.contains(&&first_segment.ident)
            {
                self.is_generic = true;
            }

            syn::visit::visit_type_path(self, type_path);
        }
    }

    let type_param_idents = generics
        .type_params()
        .map(|param| &param.ident)
        .collect::<Vec<_>>();

    let mut visitor = TypeParamVisitor {
        type_params: &type_param_idents,
        is_generic: false,
    };

    visitor.visit_type(ty);
    visitor.is_generic
}

pub(crate) fn gen_sized_family_impl(type_name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics co3::size::SizeFamily for #type_name #ty_generics #where_clause {
            type Kind = co3::size::Sized;
        }
    }
}

fn assert_no_drop(generics: &syn::Generics, ident: &syn::Ident) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        const _: () = {
            #[expect(dead_code)]
            trait AssertNoDrop {
                fn assert_no_drop();
            }

            impl #impl_generics AssertNoDrop for #ident #ty_generics #where_clause {
                fn assert_no_drop() {
                    const {
                        // TODO: This is the only place co3::impls! is used
                        // Remove it from public API when Drop is handled
                        assert!(co3::impls!(Self: !Drop));
                    }
                }
            }
        };
    }
}

fn repr_type_name(repr: &syn::Type) -> Option<&str> {
    let syn::Type::Path(type_path) = repr else {
        return None;
    };

    type_path
        .path
        .get_ident()
        .map(syn::Ident::to_string)
        .map(|s| match s.as_str() {
            "u8" => "u8",
            "i8" => "i8",
            "u16" => "u16",
            "i16" => "i16",
            "u32" => "u32",
            "i32" => "i32",
            "u64" => "u64",
            "i64" => "i64",
            _ => "",
        })
        .filter(|s| !s.is_empty())
}

pub fn repr_type_is_signed(repr: &syn::Type) -> bool {
    matches!(repr_type_name(repr), Some("i8" | "i16" | "i32" | "i64"))
}

pub(super) fn enum_tag_type(repr: Option<&ReprKind>, variants_len: usize) -> Option<syn::Type> {
    fn infer_repr(num_variants: usize) -> syn::Type {
        const U8_CAPACITY: usize = u8::MAX as usize + 1;
        const U16_CAPACITY: usize = u16::MAX as usize + 1;
        const U32_CAPACITY: usize = u32::MAX as usize + 1;

        #[expect(clippy::match_overlapping_arm)]
        match num_variants {
            0..=U8_CAPACITY => syn::parse_quote!(u8),
            0..=U16_CAPACITY => syn::parse_quote!(u16),
            0..=U32_CAPACITY => syn::parse_quote!(u32),
            // TODO: is this correct
            _ => syn::parse_quote!(u64),
        }
    }

    match repr {
        None => Some(infer_repr(variants_len)),
        Some(ReprKind::Transparent) => None,
        Some(ReprKind::C(None)) => unreachable!(),
        Some(ReprKind::C(Some(repr))) | Some(ReprKind::Primitive(repr)) => Some(*repr.clone()),
    }
}

/// Checks if an enum exhausts all possible values of its repr type
fn is_exhaustive_enum(num_variants: usize, repr: &syn::Type) -> bool {
    fn repr_type_bit_width(repr: &syn::Type) -> Option<u32> {
        match repr_type_name(repr)? {
            "u8" | "i8" => Some(8),
            "u16" | "i16" => Some(16),
            "u32" | "i32" => Some(32),
            "u64" | "i64" => Some(64),
            _ => None,
        }
    }

    let max_values = match repr_type_bit_width(repr) {
        Some(8) => 1u64 << 8,
        Some(16) => 1u64 << 16,
        Some(32) => 1u64 << 32,
        // TODO: Can we have a 128-bit platform?
        Some(64) | None | Some(_) => return false,
    };

    num_variants as u64 == max_values
}

pub fn gen_size_family_impl(
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let field_bounds = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|ty| quote! { #ty: co3::size::SizeFamily, });

    let size_kind = fields
        .last()
        .map(|field| quote! { <#field as co3::size::SizeFamily>::Kind })
        .unwrap_or_else(|| quote! { co3::size::Sized });

    quote! {
        impl #impl_generics co3::size::SizeFamily for #name #ty_generics where
            #(#field_bounds)*
            #predicates
        {
            type Kind = #size_kind;
        }
    }
}

fn generic_param_idents<'a>(
    generics: impl IntoIterator<Item = &'a syn::GenericParam>,
) -> impl Iterator<Item = TokenStream> {
    generics.into_iter().map(|param| match param {
        syn::GenericParam::Lifetime(syn::LifetimeParam { lifetime, .. }) => quote! { #lifetime },
        syn::GenericParam::Type(syn::TypeParam { ident, .. }) => quote! { #ident },
        syn::GenericParam::Const(syn::ConstParam { ident, .. }) => quote! { #ident },
    })
}
