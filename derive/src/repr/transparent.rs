use proc_macro2::TokenStream;
use quote::quote;
use syn::Index;

use super::{FfiTypeInput, FfiTypeKindAttribute};
use crate::repr::repr_c::assert_no_drop;

fn target_idx(fields: &darling::ast::Fields<super::FfiTypeField>) -> Option<usize> {
    fields.fields.first()?;
    Some(0)
}

fn gen_struct_target_access(
    fields: &darling::ast::Fields<super::FfiTypeField>,
    base: TokenStream,
) -> Option<TokenStream> {
    let target_idx = target_idx(fields)?;

    match fields.style {
        darling::ast::Style::Unit => None,
        darling::ast::Style::Tuple => {
            let idx = Index::from(target_idx);
            Some(quote! { #base.#idx })
        }
        darling::ast::Style::Struct => {
            let ident = fields.fields[target_idx].ident.as_ref()?;
            Some(quote! { #base.#ident })
        }
    }
}

fn gen_struct_constructor(
    fields: &darling::ast::Fields<super::FfiTypeField>,
    construct_path: TokenStream,
) -> Option<TokenStream> {
    let target_idx = target_idx(fields)?;

    match fields.style {
        darling::ast::Style::Unit => None,
        darling::ast::Style::Tuple => {
            let field_values = fields.fields.iter().enumerate().map(|(idx, _)| {
                if idx != target_idx {
                    quote! { Default::default() }
                } else {
                    quote! { target }
                }
            });

            Some(quote! { #construct_path(#(#field_values),*) })
        }
        darling::ast::Style::Struct => {
            let field_values = fields.fields.iter().enumerate().map(|(idx, field)| {
                let ident = field.ident.as_ref().unwrap();

                let target = if idx != target_idx {
                    quote! { Default::default() }
                } else {
                    quote! { target }
                };

                quote! { #ident: #target }
            });

            Some(quote! { #construct_path { #(#field_values),* } })
        }
    }
}

fn gen_target_pattern(fields: &darling::ast::Fields<super::FfiTypeField>) -> Option<TokenStream> {
    let target_idx = target_idx(fields)?;

    match fields.style {
        darling::ast::Style::Unit => None,
        darling::ast::Style::Tuple => {
            let field_patterns = fields.fields.iter().enumerate().map(|(idx, _)| {
                if idx == target_idx {
                    quote! { target }
                } else {
                    quote! { _ }
                }
            });

            Some(quote! { (#(#field_patterns),*) })
        }
        darling::ast::Style::Struct => {
            let ident = fields.fields[target_idx].ident.as_ref()?;
            Some(quote! { { #ident: target, .. } })
        }
    }
}

fn gen_enum_target_access(
    enum_name: &syn::Ident,
    variant_name: &syn::Ident,
    fields: &darling::ast::Fields<super::FfiTypeField>,
) -> Option<TokenStream> {
    let target_pattern = gen_target_pattern(fields)?;

    Some(quote! {
        match self {
            #enum_name::#variant_name #target_pattern => target,
        }
    })
}

fn gen_accessors(input: &FfiTypeInput) -> Option<(TokenStream, TokenStream)> {
    let name = &input.ident;

    match &input.data {
        darling::ast::Data::Struct(fields) => Some((
            gen_struct_target_access(fields, quote! { self })?,
            gen_struct_constructor(fields, quote! { #name })?,
        )),
        darling::ast::Data::Enum(variants) => {
            let variant = variants.first()?;
            let variant_name = &variant.ident;

            match variant.fields.style {
                darling::ast::Style::Unit => None,
                darling::ast::Style::Tuple | darling::ast::Style::Struct => {
                    let access_target =
                        gen_enum_target_access(name, variant_name, &variant.fields)?;

                    let construct_self = gen_struct_constructor(
                        &variant.fields,
                        quote! {
                            #name::#variant_name
                        },
                    )?;

                    Some((access_target, construct_self))
                }
            }
        }
    }
}

/// Derives FFI type for transparent items.
pub(crate) fn derive_transparent_item(input: &FfiTypeInput) -> TokenStream {
    debug_assert_eq!(
        input.repr_attr.kind.as_deref().copied(),
        Some(crate::attr::repr::ReprKind::Transparent)
    );

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let predicates = where_clause.map(|w| &w.predicates);
    let params = &input.generics.params;

    let name = &input.ident;
    let target_field = match &input.data {
        // TODO: We don't check to find which struct/enum field is not a ZST. It is just assumed that it is the first field.
        // I think something can be done inside `co3::reprC!` through the use of disjoint_impls! or via macro attribute
        darling::ast::Data::Struct(item) => item.fields.first(),
        darling::ast::Data::Enum(variants) => variants
            .first()
            .and_then(|variant| variant.fields.fields.first()),
    };

    let Some(target_field) = target_field else {
        return quote! {};
    };
    let Some((access_target, construct_self)) = gen_accessors(input) else {
        return quote! {};
    };

    let target = &target_field.ty;
    let is_valid = target_field.is_valid.as_ref().map(|is_valid| {
        quote! { (#is_valid)(target) }
    });
    let is_valid_fn = is_valid.map(|is_valid| {
        quote! {
            fn is_valid(target: &#target) -> bool {
                #is_valid
            }
        }
    });

    let custom_validation =
        if let Some(FfiTypeKindAttribute::Transparent(niche_value)) = &input.ffi_type_attr.kind {
            let niche_value = niche_value.as_ref().map(|value| {
                quote! { const NICHE_VALUE: <Self as co3::ExternC>::CType = #value; }
            });

            quote! {
                #niche_value
                #is_valid_fn
            }
        } else {
            quote! { #is_valid_fn }
        };

    let impl_drop_assert = assert_no_drop(&input.generics, name);
    let trait_ = if input.data.is_enum() {
        quote!(SizedTransparent)
    } else {
        quote!(Transparent)
    };

    let for_dummy = input
        .generics
        .params
        .is_empty()
        .then_some(quote! { for<'_dummy> });

    quote! {
        #impl_drop_assert

        co3::reprC! {
            // SAFETY: `Self` and `Self::Target` are guaranteed to be transmutable
            unsafe impl(#params) #trait_ for #name #ty_generics where (#predicates) {
                type Target = #target;

                #custom_validation
            }
        }

        impl #impl_generics co3::stored::SoftEncodeOwned for #name #ty_generics
        where
            #for_dummy #target: co3::stored::SoftEncodeOwned,
            #predicates
        {
            type Store = <#target as co3::stored::SoftEncodeOwned>::Store;

            #[inline(always)]
            fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
            where
                Self: 'itm,
            {
                co3::stored::SoftEncodeOwned::soft_encode(#access_target, store)
            }
        }

        impl<'d, #params> co3::stored::SoftDecodeOwned<'d> for #name #ty_generics
        where
            #target: co3::stored::SoftDecodeOwned<'d>,
            #predicates
        {
            type Store = <#target as co3::stored::SoftDecodeOwned<'d>>::Store;

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                co3::stored::SoftDecodeOwned::soft_decode(source, store).map(|target| #construct_self)
            }
        }

        // FIXME: This is the poorest default implementation
        // Transparent types should delegate to the inner type
        impl<#params> co3::borrow::Borrow for #name #ty_generics #where_clause
        where
            Self: Sized,
        {
            type Borrowed<'itm>
                = &'itm Self
            where
                Self: 'itm;

            type Owner = Option<Self>;

            #[inline(always)]
            fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                store.insert(self)
            }
        }

        impl<'itm, #params> co3::borrow::ToOwned<'itm> for #name #ty_generics
        where
            Self: Clone,
            #predicates
        {
            #[inline(always)]
            fn to_owned(source: Self::Borrowed<'itm>) -> Self {
                (*source).clone()
            }
        }
    }
}
