use proc_macro2::TokenStream;
use quote::quote;
use syn::Result;

use crate::utils::co3_path;

pub(crate) fn parse_tag_args(
    input: syn::parse::ParseStream<'_>,
) -> Result<(syn::Type, Option<syn::Expr>)> {
    let kind = input.parse::<syn::Type>()?;
    let value = if input.peek(syn::Token![,]) {
        input.parse::<syn::Token![,]>()?;
        input.parse::<syn::Token![unsafe]>()?;
        let content;
        syn::parenthesized!(content in input);
        let value = content.parse::<syn::Expr>()?;
        if !content.is_empty() {
            return Err(content.error("unexpected tokens after tag value"));
        }
        Some(value)
    } else {
        None
    };
    if !input.is_empty() {
        return Err(input.error("expected `#[tag(Type)]` or `#[tag(Type, unsafe(value))]`"));
    }
    Ok((kind, value))
}

pub(crate) fn derive_tag(input: &syn::DeriveInput) -> Result<TokenStream> {
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut kind = None;
    let mut value = None;

    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("tag"))
    {
        let (new_kind, new_value) = attr.parse_args_with(parse_tag_args)?;
        if kind.replace(new_kind).is_some() {
            return Err(syn::Error::new_spanned(attr, "duplicate `#[tag(...)]`"));
        }
        value = new_value;
    }

    let Some(kind) = kind else {
        let err_msg = "Tag derive requires `#[tag(Type)]` or `#[tag(Type, unsafe(value))]`";
        return Err(syn::Error::new_spanned(input, err_msg));
    };

    let co3 = co3_path();
    let ident = &input.ident;

    let tag = value.map(|value| {
        quote! {
            unsafe impl #impl_generics #co3::tag::Tagged for #ident #ty_generics #where_clause {
                const TAG: Self::Kind = #value;
            }
        }
    });

    Ok(quote! {
        impl #impl_generics #co3::tag::TagFamily for #ident #ty_generics #where_clause {
            type Kind = #kind;
        }
        #tag
    })
}
