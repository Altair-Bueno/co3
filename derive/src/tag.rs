use proc_macro2::TokenStream;
use quote::quote;
use syn::Result;

use crate::utils::co3_path;

pub(crate) fn derive_tag(input: &syn::DeriveInput) -> Result<TokenStream> {
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut id = None;
    let mut value = None;

    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("tag"))
    {
        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("unsafe") {
                return Err(meta.error("expected `unsafe(id(...))`"));
            }
            meta.parse_nested_meta(|unsafe_meta| {
                if !unsafe_meta.path.is_ident("id") {
                    return Err(unsafe_meta.error("expected `unsafe(id(...))`"));
                }
                let content;
                syn::parenthesized!(content in unsafe_meta.input);
                let new_id: syn::Type = content.parse()?;
                let new_value: Option<syn::Expr> = if content.peek(syn::Token![=]) {
                    content.parse::<syn::Token![=]>()?;
                    Some(content.parse()?)
                } else {
                    None
                };
                if id.replace(new_id).is_some() {
                    return Err(unsafe_meta.error("duplicate `id` within attribute"));
                }
                value = new_value;
                Ok(())
            })
        })?;
    }

    let Some(id) = id else {
        let err_msg = "Tag derive requires `#[tag(unsafe(id(...)))]`";
        return Err(syn::Error::new_spanned(input, err_msg));
    };

    let co3 = co3_path();
    let ident = &input.ident;

    let tag = value.map(|value| {
        quote! {
            unsafe impl #impl_generics #co3::tag::Tagged for #ident #ty_generics #where_clause {
                const ID: Self::Kind = #value;
            }
        }
    });

    Ok(quote! {
        impl #impl_generics #co3::tag::TagFamily for #ident #ty_generics #where_clause {
            type Kind = #id;
        }
        #tag
    })
}
