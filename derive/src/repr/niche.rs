use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    repr::{
        ctype::{gen_ctype_name, gen_extern_c_bounds_for_ctype},
        is_exhaustive_enum,
    },
    utils::build_extern_c_type_tuple,
};

fn gen_self_niche_bound(target: Option<TokenStream>) -> TokenStream {
    match target {
        Some(target) => quote! { Self: co3::ExternC<CType = #target>, },
        None => quote! { Self: co3::ExternC, },
    }
}

fn gen_field_niche_type_bounds(fields: &[&syn::Type]) -> TokenStream {
    let bounds = fields.iter().map(|field| {
        quote! { <#field as co3::ExternC>::CType: Copy, }
    });

    quote! { #(#bounds)* }
}

pub fn gen_struct_niche_ir(
    struct_name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    niche_value: Option<&syn::Expr>,
) -> TokenStream {
    gen_struct_niche_ir_with_mode(struct_name, generics, fields, niche_value)
}

pub fn gen_struct_niche_ir_with_mode(
    struct_name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    niche_value: Option<&syn::Expr>,
) -> TokenStream {
    let types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();

    let (impl_generics, ty_generics, _) = generics.split_for_impl();
    let predicates = generics
        .where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let ctype_name = gen_ctype_name(struct_name);
    let self_bounds = gen_self_niche_bound(Some(quote!(#ctype_name #ty_generics)));
    let field_lowering_bounds = gen_extern_c_bounds_for_ctype::<false>(generics, &types);
    let field_type_bounds = gen_field_niche_type_bounds(&types);
    let (fields_tuple, c_fields_tuple, accessors) = build_extern_c_type_tuple(&types);

    if let Some(niche_value) = niche_value {
        return quote! {
            impl #impl_generics co3::niche::Niche for #struct_name #ty_generics where
                #field_lowering_bounds
                #field_type_bounds
                #self_bounds
                #predicates
            {
                const NICHE_VALUE: Self::CType = #niche_value;
            }
        };
    }

    let for_dummy = generics
        .params
        .is_empty()
        .then_some(quote! { for<'_dummy> });

    let niche_field_values = accessors.iter().map(|accessor| {
        quote! { <#fields_tuple as co3::niche::Niche>::NICHE_VALUE.#accessor }
    });
    let niche_value = match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => {
            let field_names = fields.iter().map(|f| &f.ident);
            quote! {{ #(#field_names: #niche_field_values),* }}
        }
        syn::Fields::Unnamed(_) => {
            quote!((#(#niche_field_values),*))
        }
    };

    quote! {
        impl #impl_generics co3::niche::Niche for #struct_name #ty_generics where
            #field_lowering_bounds
            #field_type_bounds
            #self_bounds
            #for_dummy #fields_tuple: co3::niche::Niche<CType = #c_fields_tuple>,
            #predicates
        {
            const NICHE_VALUE: Self::CType = #ctype_name #niche_value;
        }
    }
}

pub fn gen_enum_niche_ir(
    repr: syn::Type,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    gen_enum_niche_ir_with_mode(repr, enum_name, generics, variants, None)
}

pub fn gen_enum_niche_ir_with_mode(
    repr: syn::Type,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    custom_niche_value: Option<&syn::Expr>,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let niche_discriminant = proc_macro2::Literal::usize_unsuffixed(variants.len());
    let is_fieldless = variants
        .iter()
        .all(|v| matches!(v.fields, syn::Fields::Unit));
    let field_types = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let niche_value = if is_fieldless {
        quote! { #niche_discriminant }
    } else {
        quote! {{
            let mut value: Self::CType = unsafe { core::mem::zeroed() };

            // SAFETY: All variant structs have tag as first field at offset 0
            // We can safely write it by casting the union pointer to the repr type
            unsafe { *<*mut Self::CType>::cast::<#repr>(core::ptr::from_mut(&mut value)) = #niche_discriminant };

            value
        }}
    };

    let self_bounds = if is_fieldless {
        gen_self_niche_bound(None)
    } else {
        quote! { Self: co3::ExternC<CType: Copy>, }
    };
    let field_lowering_bounds = gen_extern_c_bounds_for_ctype::<true>(generics, &field_types);
    let field_type_bounds = gen_field_niche_type_bounds(&field_types);

    if let Some(niche_value) = custom_niche_value {
        return quote! {
            impl #impl_generics co3::niche::Niche for #enum_name #ty_generics where
                #field_lowering_bounds
                #field_type_bounds
                #self_bounds
                #predicates
            {
                const NICHE_VALUE: <Self as co3::ExternC>::CType = #niche_value;
            }

            impl #impl_generics co3::niche::NicheFamily for #enum_name #ty_generics #where_clause {
                type Kind = co3::niche::WithCustomNiche;
            }
        };
    }

    if is_exhaustive_enum(variants.len(), &repr) {
        return quote! {
            impl #impl_generics co3::niche::NicheFamily for #enum_name #ty_generics #where_clause {
                type Kind = co3::niche::WithoutNiche;
            }
        };
    }

    quote! {
        impl #impl_generics co3::niche::Niche for #enum_name #ty_generics where
            #field_lowering_bounds
            #field_type_bounds
            #self_bounds
            #predicates
        {
            const NICHE_VALUE: <Self as co3::ExternC>::CType = #niche_value;
        }

        impl #impl_generics co3::niche::NicheFamily for #enum_name #ty_generics #where_clause {
            type Kind = co3::niche::WithCustomNiche;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::build_extern_c_type_tuple;

    fn make_types(count: usize) -> Vec<syn::Type> {
        (0..count)
            .map(|i| {
                let ident = syn::Ident::new(&format!("T{}", i), proc_macro2::Span::call_site());
                syn::parse_quote!(#ident)
            })
            .collect()
    }

    #[test]
    fn test_base_case_1_element() {
        let types = make_types(1);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, accessors) = build_extern_c_type_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (T0,)
        };
        let expected_accessors: Vec<syn::Expr> = vec![syn::parse_quote!(value.0)];

        assert_eq!(expected_accessors.len(), accessors.len());
        for (accessor, expected_accessor) in accessors.iter().zip(expected_accessors) {
            assert_eq!(expected_accessor, syn::parse_quote!(value.#accessor));
        }

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_base_case_12_elements() {
        let types = make_types(12);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, accessors) = build_extern_c_type_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11,)
        };

        let expected_accessors: Vec<syn::Expr> = vec![
            syn::parse_quote!(value.0),
            syn::parse_quote!(value.1),
            syn::parse_quote!(value.2),
            syn::parse_quote!(value.3),
            syn::parse_quote!(value.4),
            syn::parse_quote!(value.5),
            syn::parse_quote!(value.6),
            syn::parse_quote!(value.7),
            syn::parse_quote!(value.8),
            syn::parse_quote!(value.9),
            syn::parse_quote!(value.10),
            syn::parse_quote!(value.11),
        ];

        assert_eq!(expected_accessors.len(), accessors.len());
        for (accessor, expected_accessor) in accessors.iter().zip(expected_accessors) {
            assert_eq!(expected_accessor, syn::parse_quote!(value.#accessor));
        }

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_13_elements() {
        let types = make_types(13);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, accessors) = build_extern_c_type_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, ),
                (T12, ),
            )
        };

        let expected_accessors: Vec<syn::Expr> = vec![
            syn::parse_quote!(value.0.0),
            syn::parse_quote!(value.0.1),
            syn::parse_quote!(value.0.2),
            syn::parse_quote!(value.0.3),
            syn::parse_quote!(value.0.4),
            syn::parse_quote!(value.0.5),
            syn::parse_quote!(value.0.6),
            syn::parse_quote!(value.0.7),
            syn::parse_quote!(value.0.8),
            syn::parse_quote!(value.0.9),
            syn::parse_quote!(value.0.10),
            syn::parse_quote!(value.0.11),
            syn::parse_quote!(value.1.0),
        ];

        assert_eq!(expected_accessors.len(), accessors.len());
        for (accessor, expected_accessor) in accessors.iter().zip(expected_accessors) {
            assert_eq!(expected_accessor, syn::parse_quote!(value.#accessor));
        }

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_24_elements() {
        let types = make_types(24);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, accessors) = build_extern_c_type_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, ),
                (T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, ),
            )
        };

        let expected_accessors: Vec<syn::Expr> = vec![
            syn::parse_quote!(value.0.0),
            syn::parse_quote!(value.0.1),
            syn::parse_quote!(value.0.2),
            syn::parse_quote!(value.0.3),
            syn::parse_quote!(value.0.4),
            syn::parse_quote!(value.0.5),
            syn::parse_quote!(value.0.6),
            syn::parse_quote!(value.0.7),
            syn::parse_quote!(value.0.8),
            syn::parse_quote!(value.0.9),
            syn::parse_quote!(value.0.10),
            syn::parse_quote!(value.0.11),
            syn::parse_quote!(value.1.0),
            syn::parse_quote!(value.1.1),
            syn::parse_quote!(value.1.2),
            syn::parse_quote!(value.1.3),
            syn::parse_quote!(value.1.4),
            syn::parse_quote!(value.1.5),
            syn::parse_quote!(value.1.6),
            syn::parse_quote!(value.1.7),
            syn::parse_quote!(value.1.8),
            syn::parse_quote!(value.1.9),
            syn::parse_quote!(value.1.10),
            syn::parse_quote!(value.1.11),
        ];

        assert_eq!(expected_accessors.len(), accessors.len());
        for (accessor, expected_accessor) in accessors.iter().zip(expected_accessors) {
            assert_eq!(expected_accessor, syn::parse_quote!(value.#accessor));
        }

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_25_elements() {
        let types = make_types(25);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, accessors) = build_extern_c_type_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, ),
                (T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, ),
                (T24,),
            )
        };

        let expected_accessors: Vec<syn::Expr> = vec![
            syn::parse_quote!(value.0.0),
            syn::parse_quote!(value.0.1),
            syn::parse_quote!(value.0.2),
            syn::parse_quote!(value.0.3),
            syn::parse_quote!(value.0.4),
            syn::parse_quote!(value.0.5),
            syn::parse_quote!(value.0.6),
            syn::parse_quote!(value.0.7),
            syn::parse_quote!(value.0.8),
            syn::parse_quote!(value.0.9),
            syn::parse_quote!(value.0.10),
            syn::parse_quote!(value.0.11),
            syn::parse_quote!(value.1.0),
            syn::parse_quote!(value.1.1),
            syn::parse_quote!(value.1.2),
            syn::parse_quote!(value.1.3),
            syn::parse_quote!(value.1.4),
            syn::parse_quote!(value.1.5),
            syn::parse_quote!(value.1.6),
            syn::parse_quote!(value.1.7),
            syn::parse_quote!(value.1.8),
            syn::parse_quote!(value.1.9),
            syn::parse_quote!(value.1.10),
            syn::parse_quote!(value.1.11),
            syn::parse_quote!(value.2.0),
        ];

        assert_eq!(expected_accessors.len(), accessors.len());
        for (accessor, expected_accessor) in accessors.iter().zip(expected_accessors) {
            assert_eq!(expected_accessor, syn::parse_quote!(value.#accessor));
        }

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_36_elements() {
        let types = make_types(36);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, accessors) = build_extern_c_type_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, ),
                (T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, ),
                (T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, ),
            )
        };

        let expected_accessors: Vec<syn::Expr> = vec![
            syn::parse_quote!(value.0.0),
            syn::parse_quote!(value.0.1),
            syn::parse_quote!(value.0.2),
            syn::parse_quote!(value.0.3),
            syn::parse_quote!(value.0.4),
            syn::parse_quote!(value.0.5),
            syn::parse_quote!(value.0.6),
            syn::parse_quote!(value.0.7),
            syn::parse_quote!(value.0.8),
            syn::parse_quote!(value.0.9),
            syn::parse_quote!(value.0.10),
            syn::parse_quote!(value.0.11),
            syn::parse_quote!(value.1.0),
            syn::parse_quote!(value.1.1),
            syn::parse_quote!(value.1.2),
            syn::parse_quote!(value.1.3),
            syn::parse_quote!(value.1.4),
            syn::parse_quote!(value.1.5),
            syn::parse_quote!(value.1.6),
            syn::parse_quote!(value.1.7),
            syn::parse_quote!(value.1.8),
            syn::parse_quote!(value.1.9),
            syn::parse_quote!(value.1.10),
            syn::parse_quote!(value.1.11),
            syn::parse_quote!(value.2.0),
            syn::parse_quote!(value.2.1),
            syn::parse_quote!(value.2.2),
            syn::parse_quote!(value.2.3),
            syn::parse_quote!(value.2.4),
            syn::parse_quote!(value.2.5),
            syn::parse_quote!(value.2.6),
            syn::parse_quote!(value.2.7),
            syn::parse_quote!(value.2.8),
            syn::parse_quote!(value.2.9),
            syn::parse_quote!(value.2.10),
            syn::parse_quote!(value.2.11),
        ];

        assert_eq!(expected_accessors.len(), accessors.len());
        for (accessor, expected_accessor) in accessors.iter().zip(expected_accessors) {
            assert_eq!(expected_accessor, syn::parse_quote!(value.#accessor));
        }

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_37_elements() {
        let types = make_types(37);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, accessors) = build_extern_c_type_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, ),
                (T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, ),
                (T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, ),
                (T36,),
            )
        };

        let expected_accessors: Vec<syn::Expr> = vec![
            syn::parse_quote!(value.0.0),
            syn::parse_quote!(value.0.1),
            syn::parse_quote!(value.0.2),
            syn::parse_quote!(value.0.3),
            syn::parse_quote!(value.0.4),
            syn::parse_quote!(value.0.5),
            syn::parse_quote!(value.0.6),
            syn::parse_quote!(value.0.7),
            syn::parse_quote!(value.0.8),
            syn::parse_quote!(value.0.9),
            syn::parse_quote!(value.0.10),
            syn::parse_quote!(value.0.11),
            syn::parse_quote!(value.1.0),
            syn::parse_quote!(value.1.1),
            syn::parse_quote!(value.1.2),
            syn::parse_quote!(value.1.3),
            syn::parse_quote!(value.1.4),
            syn::parse_quote!(value.1.5),
            syn::parse_quote!(value.1.6),
            syn::parse_quote!(value.1.7),
            syn::parse_quote!(value.1.8),
            syn::parse_quote!(value.1.9),
            syn::parse_quote!(value.1.10),
            syn::parse_quote!(value.1.11),
            syn::parse_quote!(value.2.0),
            syn::parse_quote!(value.2.1),
            syn::parse_quote!(value.2.2),
            syn::parse_quote!(value.2.3),
            syn::parse_quote!(value.2.4),
            syn::parse_quote!(value.2.5),
            syn::parse_quote!(value.2.6),
            syn::parse_quote!(value.2.7),
            syn::parse_quote!(value.2.8),
            syn::parse_quote!(value.2.9),
            syn::parse_quote!(value.2.10),
            syn::parse_quote!(value.2.11),
            syn::parse_quote!(value.3.0),
        ];

        assert_eq!(expected_accessors.len(), accessors.len());
        for (accessor, expected_accessor) in accessors.iter().zip(expected_accessors) {
            assert_eq!(expected_accessor, syn::parse_quote!(value.#accessor));
        }

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_144_elements() {
        let types = make_types(144);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, accessors) = build_extern_c_type_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, ),
                (T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, ),
                (T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, ),
                (T36, T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47, ),
                (T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59, ),
                (T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, ),
                (T72, T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83, ),
                (T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, ),
                (T96, T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107, ),
                (T108, T109, T110, T111, T112, T113, T114, T115, T116, T117, T118, T119, ),
                (T120, T121, T122, T123, T124, T125, T126, T127, T128, T129, T130, T131, ),
                (T132, T133, T134, T135, T136, T137, T138, T139, T140, T141, T142, T143, ),
            )
        };

        let expected_accessors: Vec<syn::Expr> = vec![
            syn::parse_quote!(value.0.0),
            syn::parse_quote!(value.0.1),
            syn::parse_quote!(value.0.2),
            syn::parse_quote!(value.0.3),
            syn::parse_quote!(value.0.4),
            syn::parse_quote!(value.0.5),
            syn::parse_quote!(value.0.6),
            syn::parse_quote!(value.0.7),
            syn::parse_quote!(value.0.8),
            syn::parse_quote!(value.0.9),
            syn::parse_quote!(value.0.10),
            syn::parse_quote!(value.0.11),
            syn::parse_quote!(value.1.0),
            syn::parse_quote!(value.1.1),
            syn::parse_quote!(value.1.2),
            syn::parse_quote!(value.1.3),
            syn::parse_quote!(value.1.4),
            syn::parse_quote!(value.1.5),
            syn::parse_quote!(value.1.6),
            syn::parse_quote!(value.1.7),
            syn::parse_quote!(value.1.8),
            syn::parse_quote!(value.1.9),
            syn::parse_quote!(value.1.10),
            syn::parse_quote!(value.1.11),
            syn::parse_quote!(value.2.0),
            syn::parse_quote!(value.2.1),
            syn::parse_quote!(value.2.2),
            syn::parse_quote!(value.2.3),
            syn::parse_quote!(value.2.4),
            syn::parse_quote!(value.2.5),
            syn::parse_quote!(value.2.6),
            syn::parse_quote!(value.2.7),
            syn::parse_quote!(value.2.8),
            syn::parse_quote!(value.2.9),
            syn::parse_quote!(value.2.10),
            syn::parse_quote!(value.2.11),
            syn::parse_quote!(value.3.0),
            syn::parse_quote!(value.3.1),
            syn::parse_quote!(value.3.2),
            syn::parse_quote!(value.3.3),
            syn::parse_quote!(value.3.4),
            syn::parse_quote!(value.3.5),
            syn::parse_quote!(value.3.6),
            syn::parse_quote!(value.3.7),
            syn::parse_quote!(value.3.8),
            syn::parse_quote!(value.3.9),
            syn::parse_quote!(value.3.10),
            syn::parse_quote!(value.3.11),
            syn::parse_quote!(value.4.0),
            syn::parse_quote!(value.4.1),
            syn::parse_quote!(value.4.2),
            syn::parse_quote!(value.4.3),
            syn::parse_quote!(value.4.4),
            syn::parse_quote!(value.4.5),
            syn::parse_quote!(value.4.6),
            syn::parse_quote!(value.4.7),
            syn::parse_quote!(value.4.8),
            syn::parse_quote!(value.4.9),
            syn::parse_quote!(value.4.10),
            syn::parse_quote!(value.4.11),
            syn::parse_quote!(value.5.0),
            syn::parse_quote!(value.5.1),
            syn::parse_quote!(value.5.2),
            syn::parse_quote!(value.5.3),
            syn::parse_quote!(value.5.4),
            syn::parse_quote!(value.5.5),
            syn::parse_quote!(value.5.6),
            syn::parse_quote!(value.5.7),
            syn::parse_quote!(value.5.8),
            syn::parse_quote!(value.5.9),
            syn::parse_quote!(value.5.10),
            syn::parse_quote!(value.5.11),
            syn::parse_quote!(value.6.0),
            syn::parse_quote!(value.6.1),
            syn::parse_quote!(value.6.2),
            syn::parse_quote!(value.6.3),
            syn::parse_quote!(value.6.4),
            syn::parse_quote!(value.6.5),
            syn::parse_quote!(value.6.6),
            syn::parse_quote!(value.6.7),
            syn::parse_quote!(value.6.8),
            syn::parse_quote!(value.6.9),
            syn::parse_quote!(value.6.10),
            syn::parse_quote!(value.6.11),
            syn::parse_quote!(value.7.0),
            syn::parse_quote!(value.7.1),
            syn::parse_quote!(value.7.2),
            syn::parse_quote!(value.7.3),
            syn::parse_quote!(value.7.4),
            syn::parse_quote!(value.7.5),
            syn::parse_quote!(value.7.6),
            syn::parse_quote!(value.7.7),
            syn::parse_quote!(value.7.8),
            syn::parse_quote!(value.7.9),
            syn::parse_quote!(value.7.10),
            syn::parse_quote!(value.7.11),
            syn::parse_quote!(value.8.0),
            syn::parse_quote!(value.8.1),
            syn::parse_quote!(value.8.2),
            syn::parse_quote!(value.8.3),
            syn::parse_quote!(value.8.4),
            syn::parse_quote!(value.8.5),
            syn::parse_quote!(value.8.6),
            syn::parse_quote!(value.8.7),
            syn::parse_quote!(value.8.8),
            syn::parse_quote!(value.8.9),
            syn::parse_quote!(value.8.10),
            syn::parse_quote!(value.8.11),
            syn::parse_quote!(value.9.0),
            syn::parse_quote!(value.9.1),
            syn::parse_quote!(value.9.2),
            syn::parse_quote!(value.9.3),
            syn::parse_quote!(value.9.4),
            syn::parse_quote!(value.9.5),
            syn::parse_quote!(value.9.6),
            syn::parse_quote!(value.9.7),
            syn::parse_quote!(value.9.8),
            syn::parse_quote!(value.9.9),
            syn::parse_quote!(value.9.10),
            syn::parse_quote!(value.9.11),
            syn::parse_quote!(value.10.0),
            syn::parse_quote!(value.10.1),
            syn::parse_quote!(value.10.2),
            syn::parse_quote!(value.10.3),
            syn::parse_quote!(value.10.4),
            syn::parse_quote!(value.10.5),
            syn::parse_quote!(value.10.6),
            syn::parse_quote!(value.10.7),
            syn::parse_quote!(value.10.8),
            syn::parse_quote!(value.10.9),
            syn::parse_quote!(value.10.10),
            syn::parse_quote!(value.10.11),
            syn::parse_quote!(value.11.0),
            syn::parse_quote!(value.11.1),
            syn::parse_quote!(value.11.2),
            syn::parse_quote!(value.11.3),
            syn::parse_quote!(value.11.4),
            syn::parse_quote!(value.11.5),
            syn::parse_quote!(value.11.6),
            syn::parse_quote!(value.11.7),
            syn::parse_quote!(value.11.8),
            syn::parse_quote!(value.11.9),
            syn::parse_quote!(value.11.10),
            syn::parse_quote!(value.11.11),
        ];

        assert_eq!(expected_accessors.len(), accessors.len());
        for (accessor, expected_accessor) in accessors.iter().zip(expected_accessors) {
            assert_eq!(expected_accessor, syn::parse_quote!(value.#accessor));
        }

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    // FIXME:
    //#[test]
    //fn test_145_elements() {
    //    let types = make_types(145);
    //    let refs: Vec<_> = types.iter().collect();
    //    let (result, _, accessors) = build_extern_c_type_tuple(&refs);

    //    let expected: syn::Type = syn::parse_quote! {
    //        (
    //            (
    //                (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, ),
    //                (T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, ),
    //                (T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, ),
    //                (T36, T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47, ),
    //                (T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59, ),
    //                (T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, ),
    //                (T72, T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83, ),
    //                (T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, ),
    //                (T96, T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107, ),
    //                (T108, T109, T110, T111, T112, T113, T114, T115, T116, T117, T118, T119, ),
    //                (T120, T121, T122, T123, T124, T125, T126, T127, T128, T129, T130, T131, ),
    //                (T132, T133, T134, T135, T136, T137, T138, T139, T140, T141, T142, T143, ),
    //            ),
    //            (
    //                (T144, ),
    //            )
    //        )
    //    };

    //    let expected_accessors: Vec<syn::Expr> = vec![
    //        syn::parse_quote!(value.0.0.0),
    //        syn::parse_quote!(value.0.0.1),
    //        syn::parse_quote!(value.0.0.2),
    //        syn::parse_quote!(value.0.0.3),
    //        syn::parse_quote!(value.0.0.4),
    //        syn::parse_quote!(value.0.0.5),
    //        syn::parse_quote!(value.0.0.6),
    //        syn::parse_quote!(value.0.0.7),
    //        syn::parse_quote!(value.0.0.8),
    //        syn::parse_quote!(value.0.0.9),
    //        syn::parse_quote!(value.0.0.10),
    //        syn::parse_quote!(value.0.0.11),
    //        syn::parse_quote!(value.0.1.0),
    //        syn::parse_quote!(value.0.1.1),
    //        syn::parse_quote!(value.0.1.2),
    //        syn::parse_quote!(value.0.1.3),
    //        syn::parse_quote!(value.0.1.4),
    //        syn::parse_quote!(value.0.1.5),
    //        syn::parse_quote!(value.0.1.6),
    //        syn::parse_quote!(value.0.1.7),
    //        syn::parse_quote!(value.0.1.8),
    //        syn::parse_quote!(value.0.1.9),
    //        syn::parse_quote!(value.0.1.10),
    //        syn::parse_quote!(value.0.1.11),
    //        syn::parse_quote!(value.0.2.0),
    //        syn::parse_quote!(value.0.2.1),
    //        syn::parse_quote!(value.0.2.2),
    //        syn::parse_quote!(value.0.2.3),
    //        syn::parse_quote!(value.0.2.4),
    //        syn::parse_quote!(value.0.2.5),
    //        syn::parse_quote!(value.0.2.6),
    //        syn::parse_quote!(value.0.2.7),
    //        syn::parse_quote!(value.0.2.8),
    //        syn::parse_quote!(value.0.2.9),
    //        syn::parse_quote!(value.0.2.10),
    //        syn::parse_quote!(value.0.2.11),
    //        syn::parse_quote!(value.0.3.0),
    //        syn::parse_quote!(value.0.3.1),
    //        syn::parse_quote!(value.0.3.2),
    //        syn::parse_quote!(value.0.3.3),
    //        syn::parse_quote!(value.0.3.4),
    //        syn::parse_quote!(value.0.3.5),
    //        syn::parse_quote!(value.0.3.6),
    //        syn::parse_quote!(value.0.3.7),
    //        syn::parse_quote!(value.0.3.8),
    //        syn::parse_quote!(value.0.3.9),
    //        syn::parse_quote!(value.0.3.10),
    //        syn::parse_quote!(value.0.3.11),
    //        syn::parse_quote!(value.0.4.0),
    //        syn::parse_quote!(value.0.4.1),
    //        syn::parse_quote!(value.0.4.2),
    //        syn::parse_quote!(value.0.4.3),
    //        syn::parse_quote!(value.0.4.4),
    //        syn::parse_quote!(value.0.4.5),
    //        syn::parse_quote!(value.0.4.6),
    //        syn::parse_quote!(value.0.4.7),
    //        syn::parse_quote!(value.0.4.8),
    //        syn::parse_quote!(value.0.4.9),
    //        syn::parse_quote!(value.0.4.10),
    //        syn::parse_quote!(value.0.4.11),
    //        syn::parse_quote!(value.0.5.0),
    //        syn::parse_quote!(value.0.5.1),
    //        syn::parse_quote!(value.0.5.2),
    //        syn::parse_quote!(value.0.5.3),
    //        syn::parse_quote!(value.0.5.4),
    //        syn::parse_quote!(value.0.5.5),
    //        syn::parse_quote!(value.0.5.6),
    //        syn::parse_quote!(value.0.5.7),
    //        syn::parse_quote!(value.0.5.8),
    //        syn::parse_quote!(value.0.5.9),
    //        syn::parse_quote!(value.0.5.10),
    //        syn::parse_quote!(value.0.5.11),
    //        syn::parse_quote!(value.0.6.0),
    //        syn::parse_quote!(value.0.6.1),
    //        syn::parse_quote!(value.0.6.2),
    //        syn::parse_quote!(value.0.6.3),
    //        syn::parse_quote!(value.0.6.4),
    //        syn::parse_quote!(value.0.6.5),
    //        syn::parse_quote!(value.0.6.6),
    //        syn::parse_quote!(value.0.6.7),
    //        syn::parse_quote!(value.0.6.8),
    //        syn::parse_quote!(value.0.6.9),
    //        syn::parse_quote!(value.0.6.10),
    //        syn::parse_quote!(value.0.6.11),
    //        syn::parse_quote!(value.0.7.0),
    //        syn::parse_quote!(value.0.7.1),
    //        syn::parse_quote!(value.0.7.2),
    //        syn::parse_quote!(value.0.7.3),
    //        syn::parse_quote!(value.0.7.4),
    //        syn::parse_quote!(value.0.7.5),
    //        syn::parse_quote!(value.0.7.6),
    //        syn::parse_quote!(value.0.7.7),
    //        syn::parse_quote!(value.0.7.8),
    //        syn::parse_quote!(value.0.7.9),
    //        syn::parse_quote!(value.0.7.10),
    //        syn::parse_quote!(value.0.7.11),
    //        syn::parse_quote!(value.0.8.0),
    //        syn::parse_quote!(value.0.8.1),
    //        syn::parse_quote!(value.0.8.2),
    //        syn::parse_quote!(value.0.8.3),
    //        syn::parse_quote!(value.0.8.4),
    //        syn::parse_quote!(value.0.8.5),
    //        syn::parse_quote!(value.0.8.6),
    //        syn::parse_quote!(value.0.8.7),
    //        syn::parse_quote!(value.0.8.8),
    //        syn::parse_quote!(value.0.8.9),
    //        syn::parse_quote!(value.0.8.10),
    //        syn::parse_quote!(value.0.8.11),
    //        syn::parse_quote!(value.0.9.0),
    //        syn::parse_quote!(value.0.9.1),
    //        syn::parse_quote!(value.0.9.2),
    //        syn::parse_quote!(value.0.9.3),
    //        syn::parse_quote!(value.0.9.4),
    //        syn::parse_quote!(value.0.9.5),
    //        syn::parse_quote!(value.0.9.6),
    //        syn::parse_quote!(value.0.9.7),
    //        syn::parse_quote!(value.0.9.8),
    //        syn::parse_quote!(value.0.9.9),
    //        syn::parse_quote!(value.0.9.10),
    //        syn::parse_quote!(value.0.9.11),
    //        syn::parse_quote!(value.0.10.0),
    //        syn::parse_quote!(value.0.10.1),
    //        syn::parse_quote!(value.0.10.2),
    //        syn::parse_quote!(value.0.10.3),
    //        syn::parse_quote!(value.0.10.4),
    //        syn::parse_quote!(value.0.10.5),
    //        syn::parse_quote!(value.0.10.6),
    //        syn::parse_quote!(value.0.10.7),
    //        syn::parse_quote!(value.0.10.8),
    //        syn::parse_quote!(value.0.10.9),
    //        syn::parse_quote!(value.0.10.10),
    //        syn::parse_quote!(value.0.10.11),
    //        syn::parse_quote!(value.0.11.0),
    //        syn::parse_quote!(value.0.11.1),
    //        syn::parse_quote!(value.0.11.2),
    //        syn::parse_quote!(value.0.11.3),
    //        syn::parse_quote!(value.0.11.4),
    //        syn::parse_quote!(value.0.11.5),
    //        syn::parse_quote!(value.0.11.6),
    //        syn::parse_quote!(value.0.11.7),
    //        syn::parse_quote!(value.0.11.8),
    //        syn::parse_quote!(value.0.11.9),
    //        syn::parse_quote!(value.0.11.10),
    //        syn::parse_quote!(value.0.11.11),
    //        syn::parse_quote!(value.1.0.0),
    //    ];

    //    assert_eq!(expected_accessors.len(), accessors.len());
    //    for (accessor, expected_accessor) in accessors.iter().zip(expected_accessors) {
    //        assert_eq!(expected_accessor, syn::parse_quote!(value.#accessor));
    //    }

    //    assert_eq!(expected, syn::parse_quote!(#result));
    //}

    //#[test]
    //fn test_156_elements() {
    //    let types = make_types(156);
    //    let refs: Vec<_> = types.iter().collect();
    //    let (result, _, accessors) = build_nested_tuple(&refs);

    //    let expected: syn::Type = syn::parse_quote! {
    //        (
    //            (
    //                (T0, T1, T2, T3, T4, T5, T6, ),
    //                (T7, T8, T9, T10, T11, T12, ),
    //            ),
    //            (
    //                (T13, T14, T15, T16, T17, T18, T19, ),
    //                (T20, T21, T22, T23, T24, T25, ),
    //            ),
    //            (
    //                (T26, T27, T28, T29, T30, T31, T32, ),
    //                (T33, T34, T35, T36, T37, T38, ),
    //            ),
    //            (
    //                (T39, T40, T41, T42, T43, T44, T45, ),
    //                (T46, T47, T48, T49, T50, T51, ),
    //            ),
    //            (
    //                (T52, T53, T54, T55, T56, T57, T58, ),
    //                (T59, T60, T61, T62, T63, T64, ),
    //            ),
    //            (
    //                (T65, T66, T67, T68, T69, T70, T71, ),
    //                (T72, T73, T74, T75, T76, T77, ),
    //            ),
    //            (
    //                (T78, T79, T80, T81, T82, T83, T84, ),
    //                (T85, T86, T87, T88, T89, T90, ),
    //            ),
    //            (
    //                (T91, T92, T93, T94, T95, T96, T97, ),
    //                (T98, T99, T100, T101, T102, T103, ),
    //            ),
    //            (
    //                (T104, T105, T106, T107, T108, T109, T110, ),
    //                (T111, T112, T113, T114, T115, T116, ),
    //            ),
    //            (
    //                (T117, T118, T119, T120, T121, T122, T123, ),
    //                (T124, T125, T126, T127, T128, T129, ),
    //            ),
    //            (
    //                (T130, T131, T132, T133, T134, T135, T136, ),
    //                (T137, T138, T139, T140, T141, T142, ),
    //            ),
    //            (
    //                (T143, T144, T145, T146, T147, T148, T149, ),
    //                (T150, T151, T152, T153, T154, T155, ),
    //            ),
    //        )
    //    };

    //    let expected_accessors: Vec<syn::Expr> = vec![
    //        syn::parse_quote!(value.0.0.0),
    //        syn::parse_quote!(value.0.0.1),
    //        syn::parse_quote!(value.0.0.2),
    //        syn::parse_quote!(value.0.0.3),
    //        syn::parse_quote!(value.0.0.4),
    //        syn::parse_quote!(value.0.0.5),
    //        syn::parse_quote!(value.0.0.6),
    //        syn::parse_quote!(value.0.1.0),
    //        syn::parse_quote!(value.0.1.1),
    //        syn::parse_quote!(value.0.1.2),
    //        syn::parse_quote!(value.0.1.3),
    //        syn::parse_quote!(value.0.1.4),
    //        syn::parse_quote!(value.0.1.5),
    //        syn::parse_quote!(value.1.0.0),
    //        syn::parse_quote!(value.1.0.1),
    //        syn::parse_quote!(value.1.0.2),
    //        syn::parse_quote!(value.1.0.3),
    //        syn::parse_quote!(value.1.0.4),
    //        syn::parse_quote!(value.1.0.5),
    //        syn::parse_quote!(value.1.0.6),
    //        syn::parse_quote!(value.1.1.0),
    //        syn::parse_quote!(value.1.1.1),
    //        syn::parse_quote!(value.1.1.2),
    //        syn::parse_quote!(value.1.1.3),
    //        syn::parse_quote!(value.1.1.4),
    //        syn::parse_quote!(value.1.1.5),
    //        syn::parse_quote!(value.2.0.0),
    //        syn::parse_quote!(value.2.0.1),
    //        syn::parse_quote!(value.2.0.2),
    //        syn::parse_quote!(value.2.0.3),
    //        syn::parse_quote!(value.2.0.4),
    //        syn::parse_quote!(value.2.0.5),
    //        syn::parse_quote!(value.2.0.6),
    //        syn::parse_quote!(value.2.1.0),
    //        syn::parse_quote!(value.2.1.1),
    //        syn::parse_quote!(value.2.1.2),
    //        syn::parse_quote!(value.2.1.3),
    //        syn::parse_quote!(value.2.1.4),
    //        syn::parse_quote!(value.2.1.5),
    //        syn::parse_quote!(value.3.0.0),
    //        syn::parse_quote!(value.3.0.1),
    //        syn::parse_quote!(value.3.0.2),
    //        syn::parse_quote!(value.3.0.3),
    //        syn::parse_quote!(value.3.0.4),
    //        syn::parse_quote!(value.3.0.5),
    //        syn::parse_quote!(value.3.0.6),
    //        syn::parse_quote!(value.3.1.0),
    //        syn::parse_quote!(value.3.1.1),
    //        syn::parse_quote!(value.3.1.2),
    //        syn::parse_quote!(value.3.1.3),
    //        syn::parse_quote!(value.3.1.4),
    //        syn::parse_quote!(value.3.1.5),
    //        syn::parse_quote!(value.4.0.0),
    //        syn::parse_quote!(value.4.0.1),
    //        syn::parse_quote!(value.4.0.2),
    //        syn::parse_quote!(value.4.0.3),
    //        syn::parse_quote!(value.4.0.4),
    //        syn::parse_quote!(value.4.0.5),
    //        syn::parse_quote!(value.4.0.6),
    //        syn::parse_quote!(value.4.1.0),
    //        syn::parse_quote!(value.4.1.1),
    //        syn::parse_quote!(value.4.1.2),
    //        syn::parse_quote!(value.4.1.3),
    //        syn::parse_quote!(value.4.1.4),
    //        syn::parse_quote!(value.4.1.5),
    //        syn::parse_quote!(value.5.0.0),
    //        syn::parse_quote!(value.5.0.1),
    //        syn::parse_quote!(value.5.0.2),
    //        syn::parse_quote!(value.5.0.3),
    //        syn::parse_quote!(value.5.0.4),
    //        syn::parse_quote!(value.5.0.5),
    //        syn::parse_quote!(value.5.0.6),
    //        syn::parse_quote!(value.5.1.0),
    //        syn::parse_quote!(value.5.1.1),
    //        syn::parse_quote!(value.5.1.2),
    //        syn::parse_quote!(value.5.1.3),
    //        syn::parse_quote!(value.5.1.4),
    //        syn::parse_quote!(value.5.1.5),
    //        syn::parse_quote!(value.6.0.0),
    //        syn::parse_quote!(value.6.0.1),
    //        syn::parse_quote!(value.6.0.2),
    //        syn::parse_quote!(value.6.0.3),
    //        syn::parse_quote!(value.6.0.4),
    //        syn::parse_quote!(value.6.0.5),
    //        syn::parse_quote!(value.6.0.6),
    //        syn::parse_quote!(value.6.1.0),
    //        syn::parse_quote!(value.6.1.1),
    //        syn::parse_quote!(value.6.1.2),
    //        syn::parse_quote!(value.6.1.3),
    //        syn::parse_quote!(value.6.1.4),
    //        syn::parse_quote!(value.6.1.5),
    //        syn::parse_quote!(value.7.0.0),
    //        syn::parse_quote!(value.7.0.1),
    //        syn::parse_quote!(value.7.0.2),
    //        syn::parse_quote!(value.7.0.3),
    //        syn::parse_quote!(value.7.0.4),
    //        syn::parse_quote!(value.7.0.5),
    //        syn::parse_quote!(value.7.0.6),
    //        syn::parse_quote!(value.7.1.0),
    //        syn::parse_quote!(value.7.1.1),
    //        syn::parse_quote!(value.7.1.2),
    //        syn::parse_quote!(value.7.1.3),
    //        syn::parse_quote!(value.7.1.4),
    //        syn::parse_quote!(value.7.1.5),
    //        syn::parse_quote!(value.8.0.0),
    //        syn::parse_quote!(value.8.0.1),
    //        syn::parse_quote!(value.8.0.2),
    //        syn::parse_quote!(value.8.0.3),
    //        syn::parse_quote!(value.8.0.4),
    //        syn::parse_quote!(value.8.0.5),
    //        syn::parse_quote!(value.8.0.6),
    //        syn::parse_quote!(value.8.1.0),
    //        syn::parse_quote!(value.8.1.1),
    //        syn::parse_quote!(value.8.1.2),
    //        syn::parse_quote!(value.8.1.3),
    //        syn::parse_quote!(value.8.1.4),
    //        syn::parse_quote!(value.8.1.5),
    //        syn::parse_quote!(value.9.0.0),
    //        syn::parse_quote!(value.9.0.1),
    //        syn::parse_quote!(value.9.0.2),
    //        syn::parse_quote!(value.9.0.3),
    //        syn::parse_quote!(value.9.0.4),
    //        syn::parse_quote!(value.9.0.5),
    //        syn::parse_quote!(value.9.0.6),
    //        syn::parse_quote!(value.9.1.0),
    //        syn::parse_quote!(value.9.1.1),
    //        syn::parse_quote!(value.9.1.2),
    //        syn::parse_quote!(value.9.1.3),
    //        syn::parse_quote!(value.9.1.4),
    //        syn::parse_quote!(value.9.1.5),
    //        syn::parse_quote!(value.10.0.0),
    //        syn::parse_quote!(value.10.0.1),
    //        syn::parse_quote!(value.10.0.2),
    //        syn::parse_quote!(value.10.0.3),
    //        syn::parse_quote!(value.10.0.4),
    //        syn::parse_quote!(value.10.0.5),
    //        syn::parse_quote!(value.10.0.6),
    //        syn::parse_quote!(value.10.1.0),
    //        syn::parse_quote!(value.10.1.1),
    //        syn::parse_quote!(value.10.1.2),
    //        syn::parse_quote!(value.10.1.3),
    //        syn::parse_quote!(value.10.1.4),
    //        syn::parse_quote!(value.10.1.5),
    //        syn::parse_quote!(value.11.0.0),
    //        syn::parse_quote!(value.11.0.1),
    //        syn::parse_quote!(value.11.0.2),
    //        syn::parse_quote!(value.11.0.3),
    //        syn::parse_quote!(value.11.0.4),
    //        syn::parse_quote!(value.11.0.5),
    //        syn::parse_quote!(value.11.0.6),
    //        syn::parse_quote!(value.11.1.0),
    //        syn::parse_quote!(value.11.1.1),
    //        syn::parse_quote!(value.11.1.2),
    //        syn::parse_quote!(value.11.1.3),
    //        syn::parse_quote!(value.11.1.4),
    //        syn::parse_quote!(value.11.1.5),
    //    ];

    //    assert_eq!(expected_accessors.len(), accessors.len());
    //    for (accessor, expected_accessor) in accessors.iter().zip(expected_accessors) {
    //        assert_eq!(expected_accessor, syn::parse_quote!(value.#accessor));
    //    }

    //    assert_eq!(expected, syn::parse_quote!(#result));
    //}
}
