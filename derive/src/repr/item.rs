use core::str::FromStr as _;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, parse_quote};

use crate::repr::{
    ReprCAttrs, VariantReprCAttrs,
    attr::ReprKind,
    borrow::{
        either_variant_name, gen_borrow_cast_eq_bounds, gen_const_view_name, gen_either_name,
        gen_identity_borrow_impls, gen_item_borrow_impls, gen_item_view, gen_view_ctype_name,
        gen_view_family_impls, gen_view_owner_name,
    },
    ctype::{
        gen_ctype_name, gen_extern_c_bounds_for_ctype, gen_item_ctype, gen_variant_struct_name,
    },
    enum_tag_type, gen_size_family_impl, gen_sized_family_impl, generic_param_idents,
    is_exhaustive_enum, is_type_parameterized, repr_type_is_signed,
};

pub(super) fn derive_item(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let is_view = attrs.is_view;

    let ctype_def = (!is_view).then(|| gen_item_ctype(repr, input));
    let view_def = (!is_view).then(|| gen_item_view(input, attrs, variant_attrs));

    let family_impls = if is_view {
        gen_view_family_impls(&input.ident, &input.generics)
    } else {
        gen_item_family_impls(
            repr,
            input,
            attrs.niche_value.as_ref(),
            attrs.is_valid.as_ref(),
            variant_attrs,
        )
    };

    let borrow_impls = (!is_view).then(|| gen_item_borrow_impls(input));
    let codec_impls = gen_item_codec_impls(repr, input, attrs, variant_attrs);
    // TODO:
    //let niche_impls = gen_struct_niche_ir_with_mode(name, generics, fields, ffi_type_kind);

    let repr_c_impls = repr
        .is_some()
        .then(|| gen_item_repr_c_impls(repr, input, attrs, variant_attrs));

    quote! {
        #ctype_def
        #view_def

        #family_impls
        #borrow_impls
        #codec_impls
        //#niche_impls

        #repr_c_impls
    }
}

fn gen_item_family_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    niche_value: Option<&syn::Expr>,
    is_valid: Option<&syn::ExprClosure>,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    match &input.data {
        syn::Data::Struct(data) => gen_struct_family_impls(
            repr,
            &input.ident,
            &input.generics,
            &data.fields,
            is_valid.is_some(),
            niche_value.is_some(),
        ),
        syn::Data::Enum(data) => gen_enum_family_impls(
            repr,
            &input.ident,
            &input.generics,
            &data.variants,
            variant_attrs,
        ),
        syn::Data::Union(_) => unreachable!(),
    }
}

fn gen_item_codec_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let is_view = attrs.is_view;

    match &input.data {
        syn::Data::Struct(data) => gen_struct_codec_impls(
            is_view,
            &input.ident,
            &input.generics,
            &data.fields,
            attrs.is_valid.as_ref(),
        ),
        syn::Data::Enum(data) => gen_enum_codec_impls(
            is_view,
            repr,
            &input.ident,
            &input.generics,
            &data.variants,
            variant_attrs,
        ),
        syn::Data::Union(_) => unreachable!(),
    }
}

fn gen_item_repr_c_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let is_view = attrs.is_view;

    match &input.data {
        syn::Data::Struct(data) => gen_repr_c_struct_impls(
            is_view,
            &input.ident,
            &input.generics,
            &data.fields,
            attrs.is_valid.as_ref(),
        ),
        syn::Data::Enum(data) => gen_repr_c_data_enum_impls(
            is_view,
            repr,
            &input.ident,
            &input.generics,
            &data.variants,
            variant_attrs,
        ),
        syn::Data::Union(_) => unreachable!(),
    }
}

fn gen_struct_family_impls(
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    has_trap_values: bool,
    has_custom_niche: bool,
) -> TokenStream {
    let fields = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();

    let repr_family_impl = if repr.is_some() {
        gen_repr_family_impl(name, generics, &fields, has_trap_values)
    } else {
        gen_rust_repr_family_impl(name, generics)
    };

    let size_family_impl = gen_size_family_impl(name, generics, &fields);
    let niche_family_impl = if has_custom_niche {
        // TODO: ?Sized types shouln't allow custom niche. Add validation against it
        gen_niche_family_impl_with_kind(name, generics, quote! {co3::niche::WithCustomNiche })
    } else {
        gen_niche_family_impl(name, generics, &fields)
    };

    quote! {
        #repr_family_impl
        #size_family_impl
        #niche_family_impl
    }
}

fn gen_enum_family_impls(
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let repr_family_impl = if repr.is_some() {
        let has_trap_values = enum_has_trap_values(repr, variants, variant_attrs);
        gen_repr_family_impl(name, generics, &fields, has_trap_values)
    } else {
        gen_rust_repr_family_impl(name, generics)
    };

    let size_family_impl = if repr == Some(&ReprKind::Transparent) {
        gen_size_family_impl(name, generics, &fields)
    } else {
        // FIXME: Sometimes enums with variants are ZSTs and don't have a tag
        // This happens if all variants are uninhabited but one is ZST/fieldless.
        gen_sized_family_impl(name, generics)
    };

    let niche_family_impl = gen_enum_niche_family_impl(repr, name, generics, variants);

    quote! {
        #repr_family_impl
        #size_family_impl
        #niche_family_impl
    }
}

fn gen_struct_codec_impls(
    is_view: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
) -> TokenStream {
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();

    let encode_store = encode_store_type(fields);
    let decode_store = decode_store_type(fields);

    let (encode_impl, decode_impl) = {
        let fields_destructure = gen_fields_destructure(fields);

        let ctype_name = if is_view {
            gen_view_ctype_name(name)
        } else {
            gen_ctype_name(name)
        };

        let (encode_body, decode_body) =
            gen_record_conversion(None, quote!(Self), fields, is_valid);

        (
            quote! {
                let Self #fields_destructure = self;
                #ctype_name #encode_body
            },
            quote! {
                let #ctype_name #fields_destructure = source;
                #decode_body
            },
        )
    };

    gen_codec_impls::<false>(
        is_view,
        name,
        generics,
        &field_types,
        encode_store,
        decode_store,
        encode_impl,
        decode_impl,
    )
}

fn gen_transparent_enum_codec_impls(
    is_view: bool,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let Some(variant) = variants.first() else {
        return quote! {};
    };

    let variant_name = &variant.ident;
    let field_types = variant
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    let fields_destructure = gen_fields_destructure(&variant.fields);
    let ctype_name = if is_view {
        gen_view_ctype_name(name)
    } else {
        gen_ctype_name(name)
    };

    let custom_is_valid = variant_attrs
        .first()
        .and_then(|attrs| attrs.is_valid.as_ref());
    let encode_store = encode_store_type(&variant.fields);
    let decode_store = decode_store_type(&variant.fields);
    let (encode_body, decode_body) = gen_record_conversion(
        None,
        quote!(Self::#variant_name),
        &variant.fields,
        custom_is_valid,
    );

    gen_codec_impls::<true>(
        is_view,
        name,
        generics,
        &field_types,
        encode_store,
        decode_store,
        quote! {
            let Self::#variant_name #fields_destructure = self;
            #ctype_name #encode_body
        },
        quote! {
            let #ctype_name #fields_destructure = source;
            #decode_body
        },
    )
}

fn gen_enum_codec_impls(
    is_view: bool,
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    if repr == Some(&ReprKind::Transparent) {
        return gen_transparent_enum_codec_impls(is_view, name, generics, variants, variant_attrs);
    }

    let tag_type = enum_tag_type(repr, variants.len());

    let view_owner_name = is_view.then(|| gen_view_owner_name(name));
    let has_outer_tag = matches!(repr, Some(ReprKind::C(Some(_))));
    let has_variant_tag = tag_type.is_some() && !has_outer_tag;

    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let ctype_name = if is_view {
        gen_view_ctype_name(name)
    } else {
        gen_ctype_name(name)
    };
    let payload_name = if let Some(owner_name) = &view_owner_name {
        format_ident!("{owner_name}Payload")
    } else {
        format_ident!("{name}Payload")
    };
    let payload_name = if is_view {
        gen_const_view_name(&payload_name)
    } else {
        payload_name
    };

    let (encode_store, decode_store) = {
        let either_name = gen_either_name(variants.len());

        let field_encode_stores = variants.iter().map(|v| encode_store_type(&v.fields));
        let field_decode_stores = variants.iter().map(|v| decode_store_type(&v.fields));

        (
            quote! { core::option::Option<co3::either::#either_name<#(#field_encode_stores),*>> },
            quote! { core::option::Option<co3::either::#either_name<#(#field_decode_stores),*>> },
        )
    };

    let (variants_encode, variants_decode): (Vec<_>, Vec<_>) = variants
        .iter()
        .enumerate()
        .map(|(idx, variant)| {
            let either_ty = gen_either_name(variants.len());

            let variant_name = &variant.ident;
            let variant_struct_name = if let Some(owner_name) = &view_owner_name {
                gen_const_view_name(&gen_variant_struct_name(owner_name, variant_name))
            } else {
                gen_variant_struct_name(name, variant_name)
            };

            let store_variant = either_variant_name(idx);
            let variant_struct = quote! { #variant_struct_name };
            let tag_value = proc_macro2::Literal::usize_unsuffixed(idx);

            let destructure_fields = gen_fields_destructure(&variant.fields);
            let custom_is_valid = variant_attrs[idx].is_valid.as_ref();
            let variant_tag = has_variant_tag.then(|| quote!(#tag_value as #tag_type));

            let (encode_body, decode_body) = gen_record_conversion(
                variant_tag,
                quote!(Self::#variant_name),
                &variant.fields,
                custom_is_valid,
            );
            let decode_destructure = if has_variant_tag {
                gen_tagged_variant_destructure(variant_struct.clone(), &variant.fields)
            } else {
                quote! { let #variant_struct_name #destructure_fields = source; }
            };

            let encode_variant = if has_outer_tag {
                quote! {
                    #ctype_name {
                        tag: #tag_value as #tag_type,
                        payload: #payload_name { #variant_name: #variant_struct_name #encode_body },
                    }
                }
            } else {
                quote! {
                    #ctype_name {
                        #variant_name: #variant_struct_name #encode_body
                    }
                }
            };
            let decode_source = if has_outer_tag {
                quote! { source.payload.#variant_name }
            } else {
                quote! { source.#variant_name }
            };

            (
                quote! {
                    Self::#variant_name #destructure_fields => {
                        let co3::either::#either_ty::#store_variant(store) =
                            store.insert(co3::either::#either_ty::#store_variant(Default::default()))
                        else {
                            unreachable!()
                        };

                        #encode_variant
                    }
                },
                quote! {
                    #tag_value => {
                        let source = unsafe { #decode_source };

                        #decode_destructure
                        let co3::either::#either_ty::#store_variant(store) =
                            store.insert(co3::either::#either_ty::#store_variant(Default::default()))
                        else {
                            unreachable!()
                        };

                        #decode_body
                    }
                },
            )
        })
        .unzip();

    let (encode_impl, decode_impl) = (
        quote! {
            match self {
                #(#variants_encode,)*
            }
        },
        if has_outer_tag {
            quote! {
                match source.tag {
                    #(#variants_decode,)*
                    _ => None,
                }
            }
        } else {
            quote! {
                let repr_value = <*const _>::cast::<#tag_type>(core::ptr::from_ref(&source));

                match unsafe { *repr_value } {
                    #(#variants_decode,)*
                    _ => None,
                }
            }
        },
    );

    gen_codec_impls::<true>(
        is_view,
        name,
        generics,
        &fields,
        encode_store,
        decode_store,
        encode_impl,
        decode_impl,
    )
}

fn gen_tagged_variant_destructure(target_head: TokenStream, fields: &syn::Fields) -> TokenStream {
    let field_vars = field_vars(fields);

    match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => quote! {
            let #target_head { tag: _, #(#field_vars),* } = source;
        },
        syn::Fields::Unnamed(_) => quote! {
            let #target_head(_, #(#field_vars),*) = source;
        },
    }
}

fn gen_repr_c_struct_impls(
    is_view: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
) -> TokenStream {
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let field_vars = field_vars(fields);

    let ctype_name = if is_view {
        gen_view_ctype_name(name)
    } else {
        gen_ctype_name(name)
    };

    let destructure_target = gen_fields_destructure(fields);

    let is_valid_body = gen_record_is_valid(&field_vars, &field_types, is_valid);

    let is_valid_impl = quote! {
        let #ctype_name #destructure_target = target;
        #is_valid_body
    };

    gen_repr_c_impls::<false>(is_view, name, generics, &field_types, is_valid_impl)
}

fn gen_repr_c_data_enum_impls(
    is_view: bool,
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let tag_type = enum_tag_type(repr, variants.len());

    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let view_owner_name = is_view.then(|| gen_view_owner_name(name));
    let variant_body = variants.iter().enumerate().map(|(variant_idx, variant)| {
        let variant_name = &variant.ident;

        let variant_idx_lit = proc_macro2::Literal::usize_unsuffixed(variant_idx);
        let variant_struct_name = if let Some(owner_name) = &view_owner_name {
            gen_const_view_name(&gen_variant_struct_name(owner_name, variant_name))
        } else {
            gen_variant_struct_name(name, variant_name)
        };

        let variant_fields = variant.fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
        let variant_struct_type = quote! { #variant_struct_name };

        let field_names = field_vars(&variant.fields);
        let variant_field_names = (0..variant.fields.len())
            .map(|i| format_ident!("_{i}"))
            .collect::<Vec<_>>();

        let destructure_target = match &variant.fields {
            syn::Fields::Named(_) | syn::Fields::Unit => quote! {
                let #variant_struct_type { tag: _, #(#field_names),* } = unsafe {
                    &target.#variant_name
                };
            },
            syn::Fields::Unnamed(_) => quote! {
                let #variant_struct_type { tag: _, #(#variant_field_names: #field_names),* } = unsafe {
                    &target.#variant_name
                };
            },
        };

        let is_valid = variant_attrs[variant_idx].is_valid.as_ref();
        let is_valid_body = gen_record_is_valid(&field_names, &variant_fields, is_valid);

        quote! {
            #variant_idx_lit => {
                #destructure_target
                #is_valid_body
            }
        }
    });

    let is_valid_impl = quote! {
        let repr_value = <*const _>::cast::<#tag_type>(core::ptr::from_ref(target));

        match unsafe { *repr_value } {
            #(#variant_body,)*
            _ => false,
        }
    };

    gen_repr_c_impls::<true>(is_view, name, generics, &fields, is_valid_impl)
}

fn enum_has_trap_values(
    repr: Option<&ReprKind>,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_attrs: &[VariantReprCAttrs],
) -> bool {
    let tag = enum_tag_type(repr, variants.len());

    if !tag.is_none_or(|tag| is_exhaustive_enum(variants.len(), &tag)) {
        return false;
    }

    variants.len() == variant_attrs.len()
        && variant_attrs.iter().any(|attrs| attrs.is_valid.is_some())
}

fn gen_repr_c_impls<const ADD_COPY: bool>(
    is_view: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    is_valid_body: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let checked_transmute_bounds = gen_checked_transmute_bounds::<ADD_COPY>(generics, fields);
    let (borrow_cast_bounds, cast_eq_bounds) = if is_view {
        (
            gen_borrow_cast_view_bounds::<ADD_COPY>(generics, fields),
            gen_borrow_cast_eq_bounds(fields),
        )
    } else {
        (vec![], quote! {})
    };

    quote! {
        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #name #ty_generics where
            #(for<'_išč> &'_išč #fields: co3::Decode<'_išč>,)*
            #(#checked_transmute_bounds,)*
            #(#borrow_cast_bounds,)*
            #cast_eq_bounds
            #predicates
        {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                // FIXME:
                //#is_valid_body
                unimplemented!()
            }
        }
    }
}

fn gen_checked_transmute_bounds<const ADD_COPY: bool>(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<syn::WherePredicate> {
    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    let mut predicates = fields
        .iter()
        .map(|&ty| {
            let for_dummy = (!is_type_parameterized(ty, generics)).then_some(quote!(for<'_dummy>));

            let ctype_bound = if ADD_COPY {
                quote!(<CType: Copy>)
            } else {
                quote! {<CType: Sized>}
            };

            parse_quote! { #for_dummy #ty: co3::transmute::CheckedTransmute #ctype_bound }
        })
        .collect::<Vec<_>>();

    let for_dummy = (!is_type_parameterized(last, generics)).then_some(quote!(for<'_dummy>));
    let copy_bound = ADD_COPY.then(|| quote! { <CType: Copy> });

    predicates.push(parse_quote! {
        #for_dummy #last: co3::transmute::CheckedTransmute #copy_bound
    });

    predicates
}

fn gen_record_is_valid(
    field_names: &[Ident],
    field_types: &[&syn::Type],
    is_valid: Option<&syn::ExprClosure>,
) -> TokenStream {
    let custom_validation = is_valid.map(|is_valid| {
        quote! { && (#is_valid)(#(#field_names),*) }
    });

    quote! { #(
        let Some(#field_names) = (unsafe { <&#field_types as co3::Decode>::decode(#field_names) }) else {
            return false;
        };)*

        true #custom_validation
    }
}

pub(super) fn derive_fieldless_enum(
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;

    let repr_family = match repr {
        None => quote! { co3::ir::ReprRust },
        Some(ReprKind::C(_)) => unreachable!(),
        // NOTE: Fieldless enum with one variant is a ZST
        Some(ReprKind::Transparent) => quote!(co3::ir::Robust),
        Some(ReprKind::Primitive(tag)) => {
            let robustness = if is_exhaustive_enum(variants.len(), tag) {
                quote! { co3::ir::Robust }
            } else {
                quote! { co3::ir::NonRobust }
            };

            quote! { co3::ir::ReprC<#robustness> }
        }
    };

    let size_family_impl = if variants.is_empty() {
        unimplemented!("uninhabited types are not yet supported")
    } else {
        gen_sized_family_impl(name, generics)
    };

    let niche_family_impl = gen_enum_niche_family_impl(repr, name, generics, variants);

    let checked_transmute_method = match repr {
        None => quote! {},
        Some(ReprKind::C(_)) => unreachable!(),
        Some(ReprKind::Transparent) => {
            quote! { unsafe fn is_valid((): &()) -> bool { true } }
        }
        Some(ReprKind::Primitive(repr)) if is_exhaustive_enum(variants.len(), repr) => {
            quote! { unsafe fn is_valid(_: &Self::CType) -> bool { true } }
        }
        Some(ReprKind::Primitive(repr)) => {
            let niche_value = proc_macro2::Literal::usize_unsuffixed(variants.len());

            let is_valid = if repr_type_is_signed(repr) {
                quote! { *target >= 0 && (*target as usize) < #niche_value }
            } else {
                quote! { (*target as usize) < #niche_value }
            };

            quote! { unsafe fn is_valid(target: &Self::CType) -> bool { #is_valid } }
        }
    };

    let checked_transmute_impl = match repr {
        None => quote! {},
        Some(ReprKind::C(_)) => unreachable!(),
        Some(ReprKind::Transparent | ReprKind::Primitive(_)) => {
            quote! {
                unsafe impl #impl_generics co3::transmute::CheckedTransmute for #name #ty_generics #where_clause {
                    #[inline(always)]
                    #checked_transmute_method
                }
            }
        }
    };

    let variants_decode = variants.iter().enumerate().map(|(i, variant)| {
        let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
        let variant_name = &variant.ident;
        quote! { #idx => Some(Self::#variant_name) }
    });

    let tag_type = enum_tag_type(repr, variants.len());
    let ctype = tag_type
        .as_ref()
        .map(|repr| quote! { #repr })
        .unwrap_or_else(|| quote! { () });
    let encode_impl = tag_type
        .as_ref()
        .map(|repr| quote! { self as #repr })
        .unwrap_or_else(|| quote! { () });
    let decode_impl = if tag_type.is_some() {
        quote! {
            match source {
                #(#variants_decode,)*
                _ => None
            }
        }
    } else {
        let transparent_variant = &variants[0].ident;

        quote! {
            let () = source;
            Some(Self::#transparent_variant)
        }
    };
    // TODO:
    //let niche_ir = gen_enum_niche_ir(repr, enum_name, generics, variants);
    let borrow_impls = gen_identity_borrow_impls(name, generics);

    quote! {
        //#niche_ir
        #borrow_impls

        impl #impl_generics co3::ir::ReprFamily for #name #ty_generics #where_clause {
            type Kind = #repr_family;
        }

        #size_family_impl
        #niche_family_impl

        impl #impl_generics co3::ExternC for #name #ty_generics #where_clause {
            type CType = #ctype;
        }
        impl #impl_generics co3::stored::EncodeOwned for #name #ty_generics #where_clause {
            type Store = ();

            fn soft_encode_owned<'_išč>(self, (): &mut ()) -> Self::CType
            where
                Self: '_išč,
            {
                #encode_impl
            }
        }

        impl<'_dšč, #params> co3::stored::DecodeOwned<'_dšč> for #name #ty_generics #where_clause {
            type Store = ();

            unsafe fn soft_decode_owned<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Option<Self> {
                #decode_impl
            }
        }

        impl #impl_generics co3::Encode for #name #ty_generics #where_clause {}
        impl<#params> co3::Decode<'_> for #name #ty_generics #where_clause {}

        #checked_transmute_impl
    }
}

pub(crate) fn field_vars(fields: &syn::Fields) -> Vec<Ident> {
    let fields_cnt = fields.iter().count();

    match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => {
            fields.iter().filter_map(|f| f.ident.clone()).collect()
        }
        syn::Fields::Unnamed(_) => (0..fields_cnt).map(|i| format_ident!("_{i}")).collect(),
    }
}

pub fn gen_fields_destructure(fields: &syn::Fields) -> TokenStream {
    let field_names = field_vars(fields);

    match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => quote! {{ #(#field_names),* }},
        syn::Fields::Unnamed(_) => quote!((#(#field_names),*)),
    }
}

pub fn tuple_field_exprs(len: usize) -> Vec<TokenStream> {
    (0..len)
        .map(|i| {
            let i = syn::Index::from(i);
            quote! { &mut store.#i }
        })
        .collect()
}

fn gen_record_conversion(
    tag: Option<TokenStream>,
    source_head: TokenStream,
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
) -> (TokenStream, TokenStream) {
    let store_vars = tuple_field_exprs(fields.len());
    let field_vars = field_vars(fields);

    let tag_field = tag.as_ref().map(|tag| quote! { tag: #tag, });
    let tag_element = tag.as_ref().map(|tag| quote! { #tag, });

    let custom_validation = is_valid.map(|is_valid| {
        quote! {
            if !(#is_valid)(#(&#field_vars),*) {
                return None;
            }
        }
    });

    match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => (
            quote! {
                {
                    #tag_field
                    #(#field_vars: co3::stored::EncodeOwned::soft_encode_owned(#field_vars, #store_vars)),*
                }
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::DecodeOwned::soft_decode_owned(#field_vars, #store_vars)?
                }; )*

                #custom_validation
                Some(#source_head {
                    #(#field_vars),*
                })
            },
        ),
        syn::Fields::Unnamed(_) => (
            quote! {
                (
                    #tag_element
                    #(co3::stored::EncodeOwned::soft_encode_owned(#field_vars, #store_vars)),*
                )
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::DecodeOwned::soft_decode_owned(#field_vars, #store_vars)?
                }; )*

                #custom_validation
                Some(#source_head(
                    #(#field_vars),*
                ))
            },
        ),
    }
}

fn gen_codec_impls<const ADD_COPY: bool>(
    is_view: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    encode_store: TokenStream,
    decode_store: TokenStream,
    encode_impl: TokenStream,
    decode_impl: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let params = &generics.params;

    let extern_c_bounds = if is_view {
        Vec::new()
    } else {
        gen_extern_c_bounds_for_ctype::<ADD_COPY>(generics, fields)
    };

    let encode_owned_bounds =
        gen_field_encode_bounds(generics, fields, quote! { co3::stored::EncodeOwned });
    let decode_owned_bounds =
        gen_field_decode_bounds(generics, fields, quote! { co3::stored::DecodeOwned });

    let encode_bounds = gen_field_encode_bounds(generics, fields, quote! { co3::Encode });
    let decode_bounds = gen_field_decode_bounds(generics, fields, quote! { co3::Decode });

    let sized_bound = if is_view {
        quote! {}
    } else if !is_view && generics.params.is_empty() {
        quote!(for<'_dummy> Self: Sized,)
    } else {
        quote!(Self: Sized,)
    };

    let (decode_lifetime, borrow_cast_bounds, cast_eq_bounds) = if is_view {
        (
            quote! {},
            gen_borrow_cast_view_bounds::<ADD_COPY>(generics, fields),
            gen_borrow_cast_eq_bounds(fields),
        )
    } else {
        (quote! { '_dšč, }, vec![], quote! {})
    };

    let ctype_name = if is_view {
        gen_view_ctype_name(name)
    } else {
        gen_ctype_name(name)
    };

    let ctype_ty_generics = if is_view {
        let ty_generics = generic_param_idents(generics.params.iter().skip(1)).collect::<Vec<_>>();
        quote! { <#(#ty_generics),*> }
    } else {
        quote! { #ty_generics }
    };

    quote! {
        impl #impl_generics co3::ExternC for #name #ty_generics where
            #(#borrow_cast_bounds,)*
            #(#extern_c_bounds,)*
            #predicates
        {
            type CType = #ctype_name #ctype_ty_generics;
        }

        impl #impl_generics co3::stored::EncodeOwned for #name #ty_generics
        where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#encode_owned_bounds,)*
            #cast_eq_bounds
            #predicates
        {
            type Store = #encode_store;

            fn soft_encode_owned<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType where Self: '_išč {
                #encode_impl
            }
        }
        impl<#decode_lifetime #params> co3::stored::DecodeOwned<'_dšč> for #name #ty_generics
        where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#decode_owned_bounds,)*
            #cast_eq_bounds
            #predicates
        {
            type Store = #decode_store;

            unsafe fn soft_decode_owned<'_išč: '_dšč>(source: Self::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                #decode_impl
            }
        }

        impl #impl_generics co3::Encode for #name #ty_generics where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#encode_bounds,)*
            #cast_eq_bounds
            #predicates
        {}
        impl<#decode_lifetime #params> co3::Decode<'_dšč> for #name #ty_generics where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#decode_bounds,)*
            #cast_eq_bounds
            #predicates
        {}
    }
}

fn gen_field_encode_bounds<'a>(
    generics: &'a syn::Generics,
    fields: &'a [&syn::Type],
    bound: TokenStream,
) -> impl Iterator<Item = syn::WherePredicate> + use<'a> {
    fields.iter().map(move |ty| {
        let for_dummy = (!is_type_parameterized(ty, generics)).then_some(quote! { for<'_dummy> });
        parse_quote! { #for_dummy #ty: #bound<CType: Copy> }
    })
}

fn gen_field_decode_bounds<'a>(
    generics: &'a syn::Generics,
    fields: &'a [&syn::Type],
    bound: TokenStream,
) -> impl Iterator<Item = syn::WherePredicate> + use<'a> {
    fields.iter().map(move |ty| {
        let for_dummy = (!is_type_parameterized(ty, generics)).then_some(quote! { for<'_dummy> });
        parse_quote! { #for_dummy #ty: #bound<'_dšč, CType: Copy> }
    })
}

fn encode_store_type(fields: &syn::Fields) -> TokenStream {
    let fields = fields.iter().map(|syn::Field { ty, .. }| {
        quote! { <#ty as co3::stored::EncodeOwned>::Store }
    });

    quote!((#(#fields,)*))
}

fn decode_store_type(fields: &syn::Fields) -> TokenStream {
    let fields = fields.iter().map(|syn::Field { ty, .. }| {
        quote! { <#ty as co3::stored::DecodeOwned<'_dšč>>::Store }
    });

    quote!((#(#fields,)*))
}

fn gen_rust_repr_family_impl(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics co3::ir::ReprFamily for #name #ty_generics #where_clause {
            type Kind = co3::ir::ReprRust;
        }
    }
}

fn gen_repr_family_impl(
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    has_trap_values: bool,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let (parametrized_fields, non_parametrized_fields): (Vec<&syn::Type>, Vec<_>) = fields
        .iter()
        .partition(|ty| is_type_parameterized(ty, generics));

    let init = if has_trap_values {
        quote! { co3::ir::NonRobust }
    } else {
        quote! { co3::ir::Robust }
    };

    let mut repr_kind = quote! { co3::ir::ReprC<#init> };
    let field_bounds = parametrized_fields.iter().map(|ty| {
        quote! { #ty: co3::ir::ReprFamily }
    });

    let mut aggregate_bounds = Vec::new();
    for &field in &non_parametrized_fields {
        let kind = quote! { <#field as co3::ir::ReprFamily>::Kind };
        repr_kind = quote! { <#repr_kind as core::ops::Add<#kind>>::Output };
    }

    for &field in &parametrized_fields {
        let kind = quote! { <#field as co3::ir::ReprFamily>::Kind };
        aggregate_bounds.push(quote! { #kind: core::ops::Add<#repr_kind> });
        repr_kind = quote! { <#kind as core::ops::Add<#repr_kind>>::Output };
    }

    quote! {
        impl #impl_generics co3::ir::ReprFamily for #name #ty_generics where
            #(#field_bounds,)*
            #(#aggregate_bounds,)*
            #predicates
        {
            type Kind = #repr_kind;
        }
    }
}

fn gen_niche_family_impl(
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let sized_bound = if generics.params.is_empty() {
        quote!(for<'_dummy> Self: Sized,)
    } else {
        quote!(Self: Sized,)
    };

    let (parametrized_fields, non_parametrized_fields): (Vec<&syn::Type>, Vec<_>) = fields
        .iter()
        .partition(|ty| is_type_parameterized(ty, generics));

    // TODO: We're just using WithoutNiche for the ease of implementation. Remove it?
    let mut niche_kind = quote!(co3::niche::WithoutNiche);
    let field_bounds = parametrized_fields.iter().map(|ty| {
        quote! { #ty: co3::niche::NicheFamily }
    });

    let mut aggregate_bounds = Vec::new();
    for &field in &non_parametrized_fields {
        let kind = quote! { <#field as co3::niche::NicheFamily>::Kind };
        niche_kind = quote! { <#niche_kind as core::ops::Add<#kind>>::Output };
    }

    for &field in &parametrized_fields {
        let kind = quote! { <#field as co3::niche::NicheFamily>::Kind };

        aggregate_bounds.push(quote! { #kind: core::ops::Add<#niche_kind> });
        niche_kind = quote! { <#kind as core::ops::Add<#niche_kind>>::Output };
    }

    quote! {
        impl #impl_generics co3::niche::NicheFamily for #name #ty_generics where
            #(#field_bounds,)*
            #(#aggregate_bounds,)*
            #sized_bound
            #predicates
        {
            type Kind = #niche_kind;
        }
    }
}

fn gen_niche_family_impl_with_kind(
    item_name: &Ident,
    generics: &syn::Generics,
    kind: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let sized_bound = if generics.params.is_empty() {
        quote!(for<'_dummy> Self: Sized,)
    } else {
        quote!(Self: Sized,)
    };

    quote! {
        impl #impl_generics co3::niche::NicheFamily for #item_name #ty_generics where
            #sized_bound
            #predicates
        {
            type Kind = #kind;
        }
    }
}

fn gen_enum_niche_family_impl(
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let is_exhaustive = enum_tag_type(repr, variants.len())
        .is_none_or(|tag| is_exhaustive_enum(variants.len(), &tag));

    let niche_kind = if is_exhaustive {
        quote! { co3::niche::WithoutNiche }
    } else {
        quote! { co3::niche::WithCustomNiche }
    };

    gen_niche_family_impl_with_kind(name, generics, niche_kind)
}

fn gen_borrow_cast_view_bounds<const ADD_COPY: bool>(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<TokenStream> {
    let Some((last, fields)) = fields.split_last() else {
        return vec![];
    };

    fn borrow_ty(field_ty: &syn::Type) -> &syn::Type {
        match field_ty {
            syn::Type::Path(syn::TypePath {
                qself: Some(syn::QSelf { ty, .. }),
                ..
            }) => ty,
            _ => unreachable!(),
        }
    }

    let mut predicates = fields
        .iter()
        .map(|ty| borrow_ty(ty))
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|ty| {
            let ctype_bound = if ADD_COPY {
                quote! { <AsConst: Copy> + Copy }
            } else {
                quote! { <AsConst: Sized> }
            };

            quote! { #ty: co3::ExternC<CType: co3::borrow::BorrowCast #ctype_bound> }
        })
        .collect::<Vec<_>>();

    let last = borrow_ty(last);
    if is_type_parameterized(last, generics) {
        let ctype_bound = ADD_COPY.then(|| quote! { <AsConst: Copy> + Copy });
        predicates.push(quote!(#last: co3::ExternC<CType: co3::borrow::BorrowCast #ctype_bound>));
    }

    predicates
}
