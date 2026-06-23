use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Ident, spanned::Spanned as _, visit::Visit};

use crate::{
    generate::gen_handle_family_impl,
    repr::attr::{ReprKind, parse_repr},
    repr::item::{derive_fieldless_enum, derive_item, field_vars},
    utils::push_error,
};

mod attr;
mod borrow;
mod ctype;
mod erased;
mod item;
mod niche;

const FFI_TYPE_ATTR: &str = "reprC";

fn parse_repr_c_parts(
    attrs: &[Attribute],
) -> syn::Result<(Option<syn::Expr>, Option<syn::ExprClosure>)> {
    let Some(attr) = find_single_attr_opt(FFI_TYPE_ATTR, attrs)? else {
        return Ok((None, None));
    };

    let mut niche_value = None;
    let mut is_valid = None;
    let mut is_view = false;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("view") {
            is_view = true;
            return Ok(());
        }

        if meta.path.is_ident("NICHE_VALUE") {
            let value: syn::Expr = meta.value()?.parse()?;
            if niche_value.replace(value).is_some() {
                return Err(meta.error("Duplicate `NICHE_VALUE` within attribute"));
            }
            return Ok(());
        }

        if meta.path.is_ident("is_valid") {
            let value: syn::ExprClosure = meta.value()?.parse()?;
            if is_valid.replace(value).is_some() {
                return Err(meta.error("Duplicate `is_valid` within attribute"));
            }
            return Ok(());
        }

        Err(meta.error("unknown type kind"))
    })?;

    if niche_value.is_none() && is_valid.is_none() && !is_view {
        return Err(syn::Error::new_spanned(attr, "expected ffi type kind"));
    }

    Ok((niche_value, is_valid))
}

fn infer_struct_is_valid_from_niche(
    fields: &syn::Fields,
    niche_value: Option<&syn::Expr>,
    is_valid: &mut Option<syn::ExprClosure>,
) {
    let Some(niche_value) = niche_value else {
        return;
    };

    if is_valid.is_some() {
        return;
    }

    let field_vars = field_vars(fields);
    let niche_fields = match fields {
        syn::Fields::Unnamed(_) => (0..fields.iter().count())
            .map(|i| {
                let i = syn::Index::from(i);
                quote! { __co3_niche_value.#i }
            })
            .collect::<Vec<_>>(),
        syn::Fields::Named(_) | syn::Fields::Unit => fields
            .iter()
            .filter_map(|field| {
                let field_name = field.ident.as_ref()?;
                Some(quote! { __co3_niche_value.#field_name })
            })
            .collect::<Vec<_>>(),
    };

    *is_valid = Some(syn::parse_quote! {
        |#(#field_vars),*| {
            let __co3_niche_value = #niche_value;
            #(*#field_vars != #niche_fields)||*
        }
    });
}

pub(crate) fn derive_repr_c(input: &syn::DeriveInput) -> syn::Result<TokenStream> {
    let mut errors = None::<syn::Error>;

    let repr_attr = parse_repr(&input.attrs)?;
    let (niche_value, mut is_valid) = parse_repr_c_parts(&input.attrs)?;
    let handle_id = parse_single_list_attr_opt::<syn::Type>("id", &input.attrs)?;

    match &input.data {
        syn::Data::Struct(data) => {
            validate_fields_no_ffi_type_attr(&data.fields, &mut errors);
            infer_struct_is_valid_from_niche(&data.fields, niche_value.as_ref(), &mut is_valid);
        }
        syn::Data::Enum(data) => {
            if is_valid.is_some() {
                let err_msg = "`is_valid` is only supported on structs or enum variants";
                push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));
            }

            if niche_value.is_some() {
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

                if parse_repr_c_parts(&variant.attrs)?.0.is_some() {
                    let err_msg = "`NICHE_VALUE` is only supported on types";
                    push_error(&mut errors, syn::Error::new(variant.span(), err_msg));
                }
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
        syn::Data::Struct(_) => derive_item(
            repr_attr.as_ref(),
            input,
            niche_value.as_ref(),
            is_valid.as_ref(),
        ),
        syn::Data::Enum(data) if data.variants.is_empty() => {
            // TODO: Support uninhabited enums. yes, it is possible
            let err_msg = "Uninhabited enum is a never type. You can declare it as an opaque type in `export_!` or `extern_!` with `type Foo;`";
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
                derive_item(repr_attr.as_ref(), input, None, None)
            }
        }
        syn::Data::Union(_) => unreachable!(),
    };

    if let Some(errors) = errors {
        Err(errors)
    } else {
        let drop_impl_assert = assert_no_drop(&generics, &input.ident);

        let handle_family_impl = handle_id
            .as_ref()
            .map(|id| gen_handle_family_impl(&input.ident, &generics, id));

        Ok(quote! {
            #handle_family_impl
            #drop_impl_assert

            #tokens
        })
    }
}

/// Parses a single attribute of the form `#[attr_name(...)]`.
///
/// If no attribute with specified name is found, returns `Ok(None)`.
///
/// # Errors
///
/// - If multiple attributes with specified name are found
/// - If attribute is not a list
pub fn parse_single_list_attr_opt<Body: syn::parse::Parse>(
    attr_name: &str,
    attrs: &[syn::Attribute],
) -> syn::Result<Option<Body>> {
    let Some(attr) = find_single_attr_opt(attr_name, attrs)? else {
        return Ok(None);
    };

    match &attr.meta {
        syn::Meta::Path(_) | syn::Meta::NameValue(_) => Err(syn::Error::new_spanned(
            attr,
            format!("Expected #[{}(...)] attribute to be a list", attr_name),
        )),
        syn::Meta::List(list) => syn::parse2(list.tokens.clone()).map(Some),
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

fn is_view(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident(FFI_TYPE_ATTR))
        .any(|attr| {
            let mut is_view = false;

            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("view") {
                    is_view = true;
                }

                Ok(())
            });

            is_view
        })
}
