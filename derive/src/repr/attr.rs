//! This module provides parsing of standard rust `#[repr(...)]` attributes.

use proc_macro2::Delimiter;
use syn::{
    Attribute, Meta, Token,
    parse::{Parse, ParseStream, Parser as _},
    punctuated::Punctuated,
};

use crate::utils::push_error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReprKind {
    Transparent,
    C(Option<Box<syn::Type>>),
    Primitive(Box<syn::Type>),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Alignment {
    Aligned(syn::LitInt),
    Packed,
}

#[derive(Debug)]
enum ReprToken {
    Kind(ReprKind),
    Align(Alignment),
}

impl quote::ToTokens for ReprKind {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let tokens_ = match self {
            Self::C(None) => quote::quote! { C },
            Self::C(Some(primitive)) => quote::quote! { C, #primitive },
            Self::Primitive(primitive) => quote::quote! { #primitive },
            Self::Transparent => quote::quote! { transparent },
        };

        tokens_.to_tokens(tokens);
    }
}

impl Parse for ReprToken {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.step(|cursor| {
            let Some((ident, after_token)) = cursor.ident() else {
                return Err(cursor.error("Expected repr kind"));
            };

            let str = ident.to_string();

            match str.as_str() {
                "transparent" => Ok((ReprToken::Kind(ReprKind::Transparent), after_token)),
                "C" => Ok((ReprToken::Kind(ReprKind::C(None)), after_token)),
                "u8" => Ok((
                    ReprToken::Kind(ReprKind::Primitive(syn::parse_quote!(u8))),
                    after_token,
                )),
                "i8" => Ok((
                    ReprToken::Kind(ReprKind::Primitive(syn::parse_quote!(i8))),
                    after_token,
                )),
                "u16" => Ok((
                    ReprToken::Kind(ReprKind::Primitive(syn::parse_quote!(u16))),
                    after_token,
                )),
                "i16" => Ok((
                    ReprToken::Kind(ReprKind::Primitive(syn::parse_quote!(i16))),
                    after_token,
                )),
                "u32" => Ok((
                    ReprToken::Kind(ReprKind::Primitive(syn::parse_quote!(u32))),
                    after_token,
                )),
                "i32" => Ok((
                    ReprToken::Kind(ReprKind::Primitive(syn::parse_quote!(i32))),
                    after_token,
                )),
                "u64" => Ok((
                    ReprToken::Kind(ReprKind::Primitive(syn::parse_quote!(u64))),
                    after_token,
                )),
                "i64" => Ok((
                    ReprToken::Kind(ReprKind::Primitive(syn::parse_quote!(i64))),
                    after_token,
                )),
                "packed" => Ok((ReprToken::Align(Alignment::Packed), after_token)),
                "align"
                    if let Some((inside_of_group, _group_span, after_group)) =
                        after_token.group(Delimiter::Parenthesis) =>
                {
                    let alignment = syn::parse2::<syn::LitInt>(inside_of_group.token_stream())
                        .unwrap_or_else(|_| syn::parse_quote!(1));

                    Ok((ReprToken::Align(Alignment::Aligned(alignment)), after_group))
                }
                "align" => Ok((
                    ReprToken::Align(Alignment::Aligned(syn::parse_quote!(1))),
                    after_token,
                )),
                _ => Err(cursor.error("Unrecognized repr kind")),
            }
        })
    }
}

pub fn parse_repr(attrs: &[Attribute]) -> syn::Result<Option<ReprKind>> {
    let mut alignment: Option<Alignment> = None;
    let mut kind: Option<ReprKind> = None;
    let mut errors = None::<syn::Error>;

    let repr_attrs: Vec<_> = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("repr"))
        .collect();

    if repr_attrs.len() > 1 {
        for attr in &repr_attrs[1..] {
            push_error(
                &mut errors,
                syn::Error::new_spanned(attr, "Multiple repr attributes"),
            );
        }

        return match errors {
            Some(err) => Err(err),
            None => Ok(None),
        };
    }

    let Some(&attr) = repr_attrs.first() else {
        return Ok(None);
    };

    let Meta::List(list) = &attr.meta else {
        return Ok(None);
    };

    let tokens =
        Punctuated::<ReprToken, Token![,]>::parse_terminated.parse2(list.tokens.clone())?;

    for token in tokens {
        match token {
            ReprToken::Kind(new_kind) => match (&mut kind, new_kind) {
                (Some(ReprKind::C(None)), ReprKind::Primitive(prim)) => {
                    kind = Some(ReprKind::C(Some(prim)));
                }
                (Some(ReprKind::Primitive(prim)), ReprKind::C(None)) => {
                    kind = Some(ReprKind::C(Some(prim.clone())));
                }
                (Some(_), _) => {
                    push_error(
                        &mut errors,
                        syn::Error::new_spanned(attr, "Duplicate repr kind within attribute"),
                    );
                }
                (None, new_kind) => kind = Some(new_kind),
            },
            ReprToken::Align(new_alignment) => {
                if alignment.is_some() {
                    push_error(
                        &mut errors,
                        syn::Error::new_spanned(attr, "Duplicate repr alignment within attribute"),
                    );
                }
                alignment = Some(new_alignment);
            }
        }
    }

    match errors {
        Some(err) => Err(err),
        None => Ok(kind),
    }
}
