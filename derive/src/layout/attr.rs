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

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct Repr {
    pub kind: Option<ReprKind>,
    pub alignment: Option<syn::LitInt>,
}

#[derive(Debug)]
enum ReprToken {
    Kind(ReprKind),
    Align(syn::LitInt),
    Packed,
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
                "usize" => Ok((
                    ReprToken::Kind(ReprKind::Primitive(syn::parse_quote!(usize))),
                    after_token,
                )),
                "isize" => Ok((
                    ReprToken::Kind(ReprKind::Primitive(syn::parse_quote!(isize))),
                    after_token,
                )),
                "packed"
                    if let Some((_inside_of_group, _group_span, after_group)) =
                        after_token.group(Delimiter::Parenthesis) =>
                {
                    Ok((ReprToken::Packed, after_group))
                }
                "packed" => Ok((ReprToken::Packed, after_token)),
                "align"
                    if let Some((inside_of_group, _group_span, after_group)) =
                        after_token.group(Delimiter::Parenthesis) =>
                {
                    let alignment = syn::parse2::<syn::LitInt>(inside_of_group.token_stream())?;

                    Ok((ReprToken::Align(alignment), after_group))
                }
                "align" => Err(cursor.error("expected `align(...)`")),
                _ => Err(cursor.error("Unrecognized repr kind")),
            }
        })
    }
}

pub fn parse_repr(attrs: &[Attribute]) -> syn::Result<Repr> {
    let mut alignment = None;
    let mut kind: Option<ReprKind> = None;
    let mut errors = None::<syn::Error>;

    let repr_attrs: Vec<_> = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("repr"))
        .collect();

    if repr_attrs.is_empty() {
        return Ok(Repr::default());
    }

    for attr in repr_attrs {
        let Meta::List(list) = &attr.meta else {
            continue;
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
                    (Some(existing), new_kind) if *existing == new_kind => {}
                    (Some(_), _) => {
                        push_error(
                            &mut errors,
                            syn::Error::new_spanned(attr, "Duplicate repr kind"),
                        );
                    }
                    (None, new_kind) => kind = Some(new_kind),
                },
                ReprToken::Align(new_alignment) => {
                    let replace = match alignment.as_ref() {
                        Some(existing) => {
                            new_alignment_value(&new_alignment)? > new_alignment_value(existing)?
                        }
                        None => true,
                    };
                    if replace {
                        alignment = Some(new_alignment);
                    }
                }
                ReprToken::Packed => push_error(
                    &mut errors,
                    syn::Error::new_spanned(attr, "`repr(packed)` is not supported by `ReprC`"),
                ),
            }
        }
    }

    match errors {
        Some(err) => Err(err),
        None => Ok(Repr { kind, alignment }),
    }
}

fn new_alignment_value(alignment: &syn::LitInt) -> syn::Result<u128> {
    alignment.base10_parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_repr_parts_in_separate_attributes() {
        let attrs = [
            syn::parse_quote!(#[repr(C)]),
            syn::parse_quote!(#[repr(align(16))]),
        ];

        assert_eq!(
            parse_repr(&attrs).unwrap(),
            Repr {
                kind: Some(ReprKind::C(None)),
                alignment: Some(syn::parse_quote!(16)),
            }
        );
    }

    #[test]
    fn rejects_duplicate_repr_kinds() {
        let kind_attrs = [
            syn::parse_quote!(#[repr(C)]),
            syn::parse_quote!(#[repr(transparent)]),
        ];
        assert!(parse_repr(&kind_attrs).is_err());
    }

    #[test]
    fn accepts_matching_duplicate_repr_parts() {
        let attrs = [
            syn::parse_quote!(#[repr(C, align(16))]),
            syn::parse_quote!(#[repr(C, align(16))]),
        ];

        assert!(parse_repr(&attrs).is_ok());
    }

    #[test]
    fn keeps_the_largest_alignment() {
        let attrs = [
            syn::parse_quote!(#[repr(align(8))]),
            syn::parse_quote!(#[repr(align(16))]),
            syn::parse_quote!(#[repr(align(4))]),
        ];

        assert_eq!(
            parse_repr(&attrs).unwrap().alignment,
            Some(syn::parse_quote!(16))
        );
    }
}
