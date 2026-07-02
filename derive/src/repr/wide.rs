use proc_macro2::TokenStream;
use quote::quote;

use crate::repr::attr::ReprKind;

pub(super) fn gen_item_wide_impl(repr: Option<&ReprKind>, input: &syn::DeriveInput) -> TokenStream {
    if repr != Some(&ReprKind::Transparent) {
        return quote! {};
    }

    // NOTE: Enums can't be ?Sized even if transparent
    let syn::Data::Struct(data) = &input.data else {
        return quote! {};
    };

    let Some(field) = data.fields.iter().next() else {
        return quote! {};
    };

    gen_transparent_wide_impl(&input.ident, &input.generics, field)
}

fn gen_transparent_wide_impl(
    name: &syn::Ident,
    generics: &syn::Generics,
    field: &syn::Field,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let field_ty = &field.ty;
    let field_ref = field.ident.as_ref().map_or_else(
        || quote! { self.0 },
        |field_name| quote! { self.#field_name },
    );

    let for_dummy = if generics.params.is_empty() {
        quote! { for<'_dummy> }
    } else {
        quote! {}
    };

    quote! {
        impl #impl_generics co3::size::Wide for #name #ty_generics
        where
            #for_dummy #field_ty: co3::size::Wide,
            #predicates
        {
            type Data = <#field_ty as co3::size::Wide>::Data;
            type Metadata = <#field_ty as co3::size::Wide>::Metadata;

            #[inline(always)]
            fn metadata(&self) -> Self::Metadata {
                co3::size::Wide::metadata(&#field_ref)
            }

            #[inline(always)]
            fn as_ptr(&self) -> *const Self::Data {
                co3::size::Wide::as_ptr(&#field_ref)
            }

            #[inline(always)]
            fn as_mut_ptr(&mut self) -> *mut Self::Data {
                co3::size::Wide::as_mut_ptr(&mut #field_ref)
            }

            #[inline(always)]
            fn into_non_null(self: Box<Self>) -> core::ptr::NonNull<Self::Data> {
                let field = Box::into_raw(self) as *mut #field_ty;
                unsafe { <#field_ty as co3::size::Wide>::into_non_null(Box::from_raw(field)) }
            }

            #[inline(always)]
            unsafe fn from_raw_parts<'a>(
                data: *const Self::Data,
                metadata: Self::Metadata,
            ) -> &'a Self {
                let field = unsafe { <#field_ty as co3::size::Wide>::from_raw_parts(data, metadata) };
                unsafe { &*(field as *const #field_ty as *const Self) }
            }

            #[inline(always)]
            unsafe fn from_raw_parts_mut<'a>(
                data: *mut Self::Data,
                metadata: Self::Metadata,
            ) -> &'a mut Self {
                let field = unsafe { <#field_ty as co3::size::Wide>::from_raw_parts_mut(data, metadata) };
                unsafe { &mut *(field as *mut #field_ty as *mut Self) }
            }

            #[inline(always)]
            unsafe fn from_non_null(
                data: core::ptr::NonNull<Self::Data>,
                metadata: Self::Metadata,
            ) -> Box<Self> {
                let field = unsafe { <#field_ty as co3::size::Wide>::from_non_null(data, metadata) };
                unsafe { Box::from_raw(Box::into_raw(field) as *mut Self) }
            }
        }
    }
}
