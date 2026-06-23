use core::str::FromStr as _;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, parse_quote};

use crate::repr::{
    attr::ReprKind,
    borrow::{
        either_variant_name, gen_borrow_cast_eq_bounds, gen_const_view_name, gen_either_name,
        gen_identity_borrow_impls, gen_item_borrow_impls, gen_item_view, gen_view_ctype_name,
        gen_view_family_impls, gen_view_owner_name,
    },
    ctype::{
        gen_ctype_borrow_cast_bounds, gen_ctype_name, gen_extern_c_bounds_for_ctype,
        gen_item_ctype, gen_variant_struct_name,
    },
    enum_tag_type,
    erased::{gen_erased_item, gen_erased_name},
    gen_size_family_impl, gen_sized_family_impl, generic_param_idents, is_exhaustive_enum,
    is_type_parameterized, is_view, parse_repr_c_parts, repr_type_is_signed,
};

pub(super) fn derive_item(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    niche_value: Option<&syn::Expr>,
    is_valid: Option<&syn::ExprClosure>,
) -> TokenStream {
    let is_view = is_view(&input.attrs);

    let ctype_def = (!is_view).then(|| gen_item_ctype(repr, input));
    let view_def = (!is_view).then(|| gen_item_view(input));
    let erased_def = (!is_view).then(|| gen_erased_item(input));

    let family_impls = if is_view {
        gen_view_family_impls(&input.ident, &input.generics)
    } else {
        gen_item_family_impls(repr, input, niche_value, is_valid)
    };

    let borrow_impls = (!is_view).then(|| gen_item_borrow_impls(input));
    let codec_impls = gen_item_codec_impls(repr, input, is_valid);
    let erase_impl = (!is_view).then(|| gen_erase_impl(&input.ident, &input.generics));
    // TODO:
    //let niche_impls = gen_struct_niche_ir_with_mode(name, generics, fields, ffi_type_kind);

    let repr_c_impls = repr
        .is_some()
        .then(|| gen_item_repr_c_impls(repr, input, is_valid));

    quote! {
        #ctype_def
        #view_def
        #erased_def

        #family_impls
        #borrow_impls
        #codec_impls
        //#niche_impls

        #repr_c_impls
        #erase_impl
    }
}

fn gen_erase_impl(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let erased_name = gen_erased_name(name);
    let erase_bounds = generics.type_params().map(|param| {
        let ident = &param.ident;
        quote! { #ident: co3::handle::Erase<Erased: Sized>, }
    });

    quote! {
        unsafe impl #impl_generics co3::handle::Erase for #name #ty_generics
        where
            #(#erase_bounds)*
            #predicates
        {
            type Erased = #erased_name #ty_generics;
        }
    }
}

fn gen_item_family_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    niche_value: Option<&syn::Expr>,
    is_valid: Option<&syn::ExprClosure>,
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
        syn::Data::Enum(data) => {
            gen_enum_family_impls(repr, &input.ident, &input.generics, &data.variants)
        }
        syn::Data::Union(_) => unreachable!(),
    }
}

fn gen_item_codec_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    is_valid: Option<&syn::ExprClosure>,
) -> TokenStream {
    let is_view = is_view(&input.attrs);

    match &input.data {
        syn::Data::Struct(data) => gen_struct_codec_impls(
            is_view,
            &input.ident,
            &input.generics,
            &data.fields,
            is_valid,
        ),
        syn::Data::Enum(data) => {
            gen_enum_codec_impls(is_view, repr, &input.ident, &input.generics, &data.variants)
        }
        syn::Data::Union(_) => unreachable!(),
    }
}

fn gen_item_repr_c_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    is_valid: Option<&syn::ExprClosure>,
) -> TokenStream {
    let is_view = is_view(&input.attrs);

    match &input.data {
        syn::Data::Struct(data) => gen_repr_c_struct_impls(
            is_view,
            &input.ident,
            &input.generics,
            &data.fields,
            is_valid,
        ),
        syn::Data::Enum(data) => {
            gen_repr_c_data_enum_impls(is_view, repr, &input.ident, &input.generics, &data.variants)
        }
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
    let niche_family_impl = gen_niche_family_impl(name, generics, &fields, has_custom_niche);

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
) -> TokenStream {
    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let repr_family_impl = if repr.is_some() {
        let has_trap_values = enum_has_trap_values(repr, variants);
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
            gen_record_conversion(quote!(Self), quote!(#ctype_name), fields, is_valid);

        (
            quote! {
                let Self #fields_destructure = self;
                #encode_body
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

fn gen_enum_codec_impls(
    is_view: bool,
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let tag_type = enum_tag_type(repr, variants.len());

    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let ctype_name = if is_view {
        gen_view_ctype_name(name)
    } else {
        gen_ctype_name(name)
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

    let view_owner_name = is_view.then(|| gen_view_owner_name(name));
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

            let tag = proc_macro2::Literal::usize_unsuffixed(idx);
            let store_variant = either_variant_name(idx);
            let variant_struct = quote! { #variant_struct_name };

            // FIXME: We're unwrapping here
            let (_, custom_is_valid) = parse_repr_c_parts(&variant.attrs).unwrap();
            let destructure_fields = gen_fields_destructure(&variant.fields);
            let (encode_body, decode_body) = gen_record_conversion(
                quote!(Self::#variant_name),
                variant_struct.clone(),
                &variant.fields,
                custom_is_valid.as_ref(),
            );

            (
                quote! {
                    Self::#variant_name #destructure_fields => {
                        let co3::either::#either_ty::#store_variant(store) =
                            store.insert(co3::either::#either_ty::#store_variant(Default::default()))
                        else {
                            unreachable!()
                        };

                        #ctype_name {
                            #variant_name: #encode_body
                        }
                    }
                },
                quote! {
                    #tag => {
                        let source = unsafe { source.#variant_name };

                        let #variant_struct #destructure_fields = source;
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
        quote! {
            let repr_value = <*const _>::cast::<#tag_type>(core::ptr::from_ref(&source));

            match unsafe { *repr_value } {
                #(#variants_decode,)*
                _ => None,
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
) -> TokenStream {
    let tag_type = enum_tag_type(repr, variants.len());

    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let view_owner_name = is_view.then(|| gen_view_owner_name(name));
    let variant_body = variants.iter().enumerate().map(|(variant_idx, variant)| {
        let variant_name = &variant.ident;

        let variant_idx = proc_macro2::Literal::usize_unsuffixed(variant_idx);
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

        // FIXME: We're unwrapping here
        let (_, is_valid) = parse_repr_c_parts(&variant.attrs).unwrap();
        let is_valid_body = gen_record_is_valid(&field_names, &variant_fields, is_valid.as_ref());

        quote! {
            #variant_idx => {
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
) -> bool {
    let tag = enum_tag_type(repr, variants.len());

    if !tag.is_none_or(|tag| is_exhaustive_enum(variants.len(), &tag)) {
        return false;
    }

    variants.iter().any(|variant| {
        // FIXME: We're unwrapping here
        parse_repr_c_parts(&variant.attrs).unwrap().1.is_some()
    })
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
            gen_borrow_cast_view_bounds(generics, fields),
            gen_borrow_cast_eq_bounds(fields),
        )
    } else {
        (vec![], quote! {})
    };

    quote! {
        // TODO: This must also have ExternC<CType: Copy> bounds if it doesn't
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

            parse_quote! {
                #for_dummy #ty: co3::transmute::CheckedTransmute<CType: Copy>
            }
        })
        .collect::<Vec<_>>();

    if ADD_COPY {
        let for_dummy = (!is_type_parameterized(last, generics)).then_some(quote!(for<'_dummy>));

        predicates.push(parse_quote! {
            #for_dummy #last: co3::transmute::CheckedTransmute
        });
    }

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
    let erase_impl = gen_erase_impl(name, generics);

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

            quote! { co3::ir::Transmuted<#robustness> }
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
        impl #impl_generics co3::stored::SoftEncodeOwned for #name #ty_generics #where_clause {
            type Store = ();

            fn soft_encode<'_išč>(self, (): &mut ()) -> Self::CType
            where
                Self: '_išč,
            {
                #encode_impl
            }
        }

        impl<'_dšč, #params> co3::stored::SoftDecodeOwned<'_dšč> for #name #ty_generics #where_clause {
            type Store = ();

            unsafe fn soft_decode<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Option<Self> {
                #decode_impl
            }
        }

        impl #impl_generics co3::SoftEncode for #name #ty_generics #where_clause {}
        impl<#params> co3::SoftDecode<'_> for #name #ty_generics #where_clause {}

        #erase_impl

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
    source_head: TokenStream,
    target_head: TokenStream,
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
) -> (TokenStream, TokenStream) {
    let store_vars = tuple_field_exprs(fields.len());
    let field_vars = field_vars(fields);

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
                #target_head {#(
                    #field_vars: co3::stored::SoftEncodeOwned::soft_encode(#field_vars, #store_vars)),*
                }
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::SoftDecodeOwned::soft_decode(#field_vars, #store_vars)?
                }; )*

                #custom_validation
                Some(#source_head {
                    #(#field_vars),*
                })
            },
        ),
        syn::Fields::Unnamed(_) => (
            quote! {
                #target_head(#(
                    co3::stored::SoftEncodeOwned::soft_encode(#field_vars, #store_vars)),*
                )
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::SoftDecodeOwned::soft_decode(#field_vars, #store_vars)?
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

    let extern_c_bounds =
        (!is_view).then(|| gen_extern_c_bounds_for_ctype::<ADD_COPY>(generics, fields));

    let encode_owned_bounds =
        gen_field_encode_bounds(generics, fields, quote! { co3::stored::SoftEncodeOwned });
    let decode_owned_bounds =
        gen_field_decode_bounds(generics, fields, quote! { co3::stored::SoftDecodeOwned });

    let encode_bounds = gen_field_encode_bounds(generics, fields, quote! { co3::SoftEncode });
    let decode_bounds = gen_field_decode_bounds(generics, fields, quote! { co3::SoftDecode });

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
            gen_borrow_cast_view_bounds(generics, fields),
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
            #extern_c_bounds
            #predicates
        {
            type CType = #ctype_name #ctype_ty_generics;
        }

        impl #impl_generics co3::stored::SoftEncodeOwned for #name #ty_generics
        where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#encode_owned_bounds,)*
            #cast_eq_bounds
            #predicates
        {
            type Store = #encode_store;

            fn soft_encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType where Self: '_išč {
                #encode_impl
            }
        }
        impl<#decode_lifetime #params> co3::stored::SoftDecodeOwned<'_dšč> for #name #ty_generics
        where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#decode_owned_bounds,)*
            #cast_eq_bounds
            #predicates
        {
            type Store = #decode_store;

            unsafe fn soft_decode<'_išč: '_dšč>(source: Self::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                #decode_impl
            }
        }

        impl #impl_generics co3::SoftEncode for #name #ty_generics where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#encode_bounds,)*
            #cast_eq_bounds
            #predicates
        {}
        impl<#decode_lifetime #params> co3::SoftDecode<'_dšč> for #name #ty_generics where
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
        quote! { <#ty as co3::stored::SoftEncodeOwned>::Store }
    });

    quote!((#(#fields,)*))
}

fn decode_store_type(fields: &syn::Fields) -> TokenStream {
    let fields = fields.iter().map(|syn::Field { ty, .. }| {
        quote! { <#ty as co3::stored::SoftDecodeOwned<'_dšč>>::Store }
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

    let field_bounds = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|ty| quote! { #ty: co3::ir::ReprFamily, });

    let init = if has_trap_values {
        quote! { co3::ir::NonRobust }
    } else {
        quote! { co3::ir::Robust }
    };

    let mut aggregate_bounds = Vec::new();
    let mut repr_kind = quote! { co3::ir::Transmuted<#init> };

    for field in fields {
        let kind = quote! { <#field as co3::ir::ReprFamily>::Kind };
        let trait_ = quote! { core::ops::Add<#kind> };

        aggregate_bounds.push(quote! { #repr_kind: #trait_, });
        repr_kind = quote! { <#repr_kind as #trait_>::Output };
    }

    quote! {
        impl #impl_generics co3::ir::ReprFamily for #name #ty_generics where
            #(#field_bounds)*
            #(#aggregate_bounds)*
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
    has_custom_niche: bool,
) -> TokenStream {
    let (niche_kind, aggregate_bounds) = if has_custom_niche {
        // TODO: ?Sized types shouln't allow custom niche
        (quote! { co3::niche::WithCustomNiche }, quote! {})
    } else {
        let mut aggregate_bounds = Vec::new();

        let Some(&last) = fields.last() else {
            let niche = quote! { co3::niche::WithoutNiche };
            return gen_niche_family_impl_with_kind(name, generics, fields, niche, quote! {});
        };

        let for_dummy = (!is_type_parameterized(last, generics)).then_some(quote!(for<'_dummy>));
        let mut niche_kind = quote! { <#last as co3::niche::NicheFamily>::Kind };

        for &field in fields.iter().rev().skip(1) {
            let field_kind = quote! { <#field as co3::niche::NicheFamily>::Kind };
            let trait_ = quote! { core::ops::Add<#niche_kind> };

            aggregate_bounds.push(quote! { #for_dummy #field_kind: #trait_, });
            niche_kind = quote! { <#field_kind as #trait_>::Output };
        }

        (niche_kind, quote! { #(#aggregate_bounds)* })
    };

    gen_niche_family_impl_with_kind(name, generics, fields, niche_kind, aggregate_bounds)
}

fn gen_niche_family_impl_with_kind(
    item_name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    kind: TokenStream,
    extra_bounds: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let sized_bound = if generics.params.is_empty() {
        quote!(for<'_dummy> Self: Sized,)
    } else {
        quote!(Self: Sized,)
    };

    let mut field_bounds = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|ty| quote! { #ty: co3::niche::NicheFamily })
        .collect::<Vec<_>>();

    if let Some(last_field) = fields.last()
        && !is_type_parameterized(last_field, generics)
    {
        field_bounds.push(quote! { for<'_dummy> #last_field: co3::niche::NicheFamily });
    }

    quote! {
        impl #impl_generics co3::niche::NicheFamily for #item_name #ty_generics where
            #(#field_bounds,)*
            #extra_bounds
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
    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();
    let is_exhaustive = enum_tag_type(repr, variants.len())
        .is_none_or(|tag| is_exhaustive_enum(variants.len(), &tag));

    let niche_kind = if is_exhaustive {
        quote! { co3::niche::WithoutNiche }
    } else {
        quote! { co3::niche::WithCustomNiche }
    };

    gen_niche_family_impl_with_kind(name, generics, &fields, niche_kind, quote! {})
}

fn gen_borrow_cast_view_bounds(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<syn::WherePredicate> {
    let owned_field_tys = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|field_ty| match field_ty {
            syn::Type::Path(syn::TypePath {
                qself: Some(syn::QSelf { ty, .. }),
                ..
            }) => parse_quote!(<#ty as co3::ExternC>::CType),
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();

    gen_ctype_borrow_cast_bounds(generics, &owned_field_tys, quote!(<AsConst: Copy>))
}
