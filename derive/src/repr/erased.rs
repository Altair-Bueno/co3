use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, parse_quote};

use crate::repr::is_type_parameterized;

pub(super) fn gen_erased_item(input: &DeriveInput) -> TokenStream {
    let mut erased_def = input.clone();

    erased_def.ident = gen_erased_name(&input.ident);

    rewrite_erased_generics(&mut erased_def);
    rewrite_erased_fields(&mut erased_def.data);

    let erased_family_impls = gen_erased_family_impls(&erased_def.ident, &erased_def.generics);

    quote::quote! {
        #[doc(hidden)]
        #erased_def

        #erased_family_impls
    }
}

pub(super) fn gen_erased_name(name: &syn::Ident) -> syn::Ident {
    format_ident!("{name}Erased")
}

fn rewrite_erased_generics(input: &mut DeriveInput) {
    let field_tys: Vec<_> = match &input.data {
        syn::Data::Struct(data) => data.fields.iter().map(|f| f.ty.clone()).collect(),
        syn::Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|v| v.fields.iter().map(|f| f.ty.clone()))
            .collect(),
        syn::Data::Union(_) => unreachable!(),
    };

    for field_ty in field_tys {
        let sized = if is_type_parameterized(&field_ty, &input.generics) {
            quote!(<Erased: Sized>)
        } else {
            quote!()
        };

        input
            .generics
            .make_where_clause()
            .predicates
            .push(parse_quote! { #field_ty: co3::handle::Erase #sized });
    }
}

fn rewrite_erased_fields(data: &mut syn::Data) {
    match data {
        syn::Data::Struct(data) => rewrite_erased_field_tys(&mut data.fields),
        syn::Data::Enum(data) => {
            for variant in &mut data.variants {
                rewrite_erased_field_tys(&mut variant.fields);
            }
        }
        syn::Data::Union(_) => unreachable!(),
    }
}

fn rewrite_erased_field_tys(fields: &mut syn::Fields) {
    for field in fields {
        let ty = &field.ty;

        field.ty = parse_quote! {
            <#ty as co3::handle::Erase>::Erased
        };
    }
}

fn gen_erased_family_impls(name: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let owner_name = gen_erased_owner_name(name);
    let for_dummy = if generics.params.is_empty() {
        quote! { for<'_dummy> }
    } else {
        quote! {}
    };

    quote! {
        impl #impl_generics co3::ir::ReprFamily for #name #ty_generics
        where
            #owner_name #ty_generics: co3::ir::ReprFamily,
            #predicates
        {
            type Kind = <#owner_name #ty_generics as co3::ir::ReprFamily>::Kind;
        }

        impl #impl_generics co3::size::SizeFamily for #name #ty_generics
        where
            #owner_name #ty_generics: co3::size::SizeFamily,
            #predicates
        {
            type Kind = <#owner_name #ty_generics as co3::size::SizeFamily>::Kind;
        }

        impl #impl_generics co3::niche::NicheFamily for #name #ty_generics
        where
            #for_dummy #owner_name #ty_generics: co3::niche::NicheFamily,
            #for_dummy Self: Sized,
            #predicates
        {
            type Kind = <#owner_name #ty_generics as co3::niche::NicheFamily>::Kind;
        }
    }
}

fn gen_erased_owner_name(erased_name: &syn::Ident) -> syn::Ident {
    let erased_name_str = erased_name.to_string();
    let owned_name = erased_name_str
        .strip_suffix("Erased")
        .expect("gen_erased_owner_name called for non-erased item");

    syn::Ident::new(owned_name, erased_name.span())
}
