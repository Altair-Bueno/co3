use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::{ReprKind, is_type_parametrized};

fn gen_data_struct_name(ident: &syn::Ident) -> syn::Ident {
    format_ident!("{}Data", ident)
}

pub(crate) fn expand(
    input: &syn::DeriveInput,
    repr: Option<&ReprKind>,
) -> syn::Result<TokenStream> {
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let syn::Data::Struct(data) = &input.data else {
        return Ok(quote! {});
    };

    let Some((field, field_ref, field_member)) = last_field(&data.fields) else {
        return Ok(quote! {});
    };

    validate_last_field(repr, &data.fields, field)?;

    let name = &input.ident;
    let field_ty = &field.ty;

    let data_ty = gen_data_ty(input);
    let methods = gen_dst_methods(field_ty, field_ref, field_member);
    let alloc_methods = gen_alloc_methods();

    let wide_predicate = wide_predicate(field_ty, &input.generics);
    let data_def = gen_data_def(input, &data.fields, &wide_predicate);

    Ok(quote! {
        #data_def

        impl #impl_generics co3::wide::Wide for #name #ty_generics
        where
            #wide_predicate,
            #predicates
        {
            type Data = #data_ty;
            type Metadata = <#field_ty as co3::wide::Wide>::Metadata;

            #methods
            #alloc_methods
        }
    })
}

fn validate_last_field(
    repr: Option<&ReprKind>,
    fields: &syn::Fields,
    field: &syn::Field,
) -> syn::Result<()> {
    let err_msg = "single-field DST structs require #[repr(transparent)]";

    if fields.len() == 1
        && !matches!(repr, Some(ReprKind::Transparent))
        && matches!(field.ty, syn::Type::Slice(_) | syn::Type::TraitObject(_))
    {
        return Err(syn::Error::new_spanned(field, err_msg));
    }

    Ok(())
}

fn last_field(fields: &syn::Fields) -> Option<(&syn::Field, TokenStream, TokenStream)> {
    let (index, field) = fields.iter().enumerate().last()?;

    let (field_ref, field_member) = field.ident.as_ref().map_or_else(
        || {
            let index = syn::Index::from(index);
            (quote! { self.#index }, quote! { #index })
        },
        |field_name| (quote! { self.#field_name }, quote! { #field_name }),
    );

    Some((field, field_ref, field_member))
}

fn gen_data_ty(input: &syn::DeriveInput) -> TokenStream {
    let name = gen_data_struct_name(&input.ident);

    let args = input.generics.params.iter().map(|param| match param {
        syn::GenericParam::Lifetime(param) => {
            let lifetime = &param.lifetime;
            quote! { #lifetime }
        }
        syn::GenericParam::Type(param) => {
            let ident = &param.ident;
            quote! { #ident }
        }
        syn::GenericParam::Const(param) => {
            let ident = &param.ident;
            quote! { #ident }
        }
    });

    quote! { #name <#(#args),*> }
}

fn gen_data_def(
    input: &syn::DeriveInput,
    fields: &syn::Fields,
    wide_predicate: &TokenStream,
) -> TokenStream {
    let name = gen_data_struct_name(&input.ident);

    let attrs = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("repr") || attr.path().is_ident("cfg"));

    let vis = &input.vis;
    let fields_len = fields.len();

    let (impl_generics, _, where_clause) = input.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let suffix = match fields {
        syn::Fields::Named(fields) => {
            let fields = fields.named.iter().enumerate().map(|(index, field)| {
                let vis = &field.vis;
                let name = &field.ident;

                let is_last_field = index == fields_len - 1;
                let ty = data_field_ty(&field.ty, is_last_field);

                quote! { #vis #name: #ty }
            });

            quote! {
                where #wide_predicate, #predicates
                { #(#fields,)* }
            }
        }
        syn::Fields::Unnamed(fields) => {
            let fields = fields.unnamed.iter().enumerate().map(|(index, field)| {
                let vis = &field.vis;

                let is_last_field = index == fields_len - 1;
                let ty = data_field_ty(&field.ty, is_last_field);

                quote! { #vis #ty }
            });

            quote! {
                ( #(#fields,)*)
                where #wide_predicate, #predicates;
            }
        }
        syn::Fields::Unit => quote! {;},
    };

    quote! {
        #(#attrs)*
        #vis struct #name #impl_generics #suffix
    }
}

fn data_field_ty(ty: &syn::Type, is_last_field: bool) -> TokenStream {
    if !is_last_field {
        return quote! { #ty };
    }

    match ty {
        syn::Type::Slice(_) => quote! { [<#ty as co3::wide::Wide>::Data; 0] },
        _ => quote! { <#ty as co3::wide::Wide>::Data },
    }
}

fn wide_predicate(field_ty: &syn::Type, generics: &syn::Generics) -> TokenStream {
    let for_dummy = (!is_type_parametrized(field_ty, generics)).then(|| quote! { for<'__dummy> });
    quote! { #for_dummy #field_ty: co3::wide::Wide }
}

fn gen_dst_methods(
    field_ty: &syn::Type,
    field_ref: TokenStream,
    field_member: TokenStream,
) -> TokenStream {
    quote! {
        #[inline(always)]
        fn metadata(&self) -> Self::Metadata {
            co3::wide::Wide::metadata(&#field_ref)
        }

        #[inline(always)]
        fn as_ptr(&self) -> *const Self::Data {
            self as *const Self as *const Self::Data
        }

        #[inline(always)]
        fn as_mut_ptr(&mut self) -> *mut Self::Data {
            self as *mut Self as *mut Self::Data
        }

        #[inline(always)]
        unsafe fn from_raw_parts<'__rust_spec>(
            data: *const Self::Data,
            metadata: Self::Metadata,
        ) -> &'__rust_spec Self {
            let offset = core::mem::offset_of!(Self::Data, #field_member);
            let field = unsafe {
                <#field_ty as co3::wide::Wide>::from_raw_parts(
                    data.cast::<u8>().byte_add(offset).cast(),
                    metadata,
                )
            };
            let field_ptr = field as *const #field_ty;
            let ptr = unsafe { (field_ptr as *const Self).byte_sub(offset) };

            unsafe { &*ptr }
        }

        #[inline(always)]
        unsafe fn from_raw_parts_mut<'__rust_spec>(
            data: *mut Self::Data,
            metadata: Self::Metadata,
        ) -> &'__rust_spec mut Self {
            let offset = core::mem::offset_of!(Self::Data, #field_member);
            let field = unsafe {
                <#field_ty as co3::wide::Wide>::from_raw_parts_mut(
                    data.cast::<u8>().byte_add(offset).cast(),
                    metadata,
                )
            };
            let field_ptr = field as *mut #field_ty;
            let ptr = unsafe { (field_ptr as *mut Self).byte_sub(offset) };

            unsafe { &mut *ptr }
        }
    }
}

fn gen_alloc_methods() -> TokenStream {
    if !cfg!(feature = "alloc") {
        return quote! {};
    }

    quote! {
        #[inline(always)]
        fn into_non_null(self: Box<Self>) -> core::ptr::NonNull<Self::Data> {
            unsafe { core::ptr::NonNull::new_unchecked(Box::into_raw(self) as *mut Self::Data) }
        }

        #[inline(always)]
        unsafe fn from_non_null(
            data: core::ptr::NonNull<Self::Data>,
            metadata: Self::Metadata,
        ) -> Box<Self> {
            let ptr = unsafe {
                <Self as co3::wide::Wide>::from_raw_parts_mut(data.as_ptr(), metadata)
            } as *mut Self;
            unsafe { Box::from_raw(ptr) }
        }
    }
}
