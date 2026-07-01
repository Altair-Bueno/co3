use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, parse_quote};

use crate::repr::is_type_parameterized;

pub(super) fn gen_erased_item(input: &DeriveInput) -> TokenStream {
    let mut erased_def = input.clone();

    erased_def.attrs.retain(|attr| !attr.path().is_ident("reprC"));
    rewrite_erased_generics(&mut erased_def);
    rewrite_erased_fields(&mut erased_def.data);

    let name = core::mem::replace(&mut erased_def.ident, gen_erased_name(&input.ident));
    let erased_family_impls = gen_erased_family_impls(&name, &erased_def.generics);

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
    for bound in gen_erased_field_bounds(input) {
        input
            .generics
            .make_where_clause()
            .predicates
            .push(parse_quote! { #bound });
    }
}

pub(super) fn gen_erased_field_bounds(input: &DeriveInput) -> Vec<TokenStream> {
    let fields = field_tys(&input.data);

    let Some((last_field, fields)) = fields.split_last() else {
        return vec![];
    };

    let mut predicates: Vec<TokenStream> = fields
        .iter()
        .map(|ty| {
            let sized_bound = is_type_parameterized(ty, &input.generics).then(|| {
                quote! { <Erased: Sized> }
            });

            quote! { #ty: co3::handle::Erase #sized_bound }
        })
        .collect();

    if is_type_parameterized(last_field, &input.generics) {
        let sized_bound = (!matches!(&input.data, syn::Data::Struct(_))).then(|| {
            quote! { <Erased: Sized> }
        });

        predicates.push(quote! { #last_field: co3::handle::Erase #sized_bound });
    }

    predicates
}

fn field_tys(data: &syn::Data) -> Vec<syn::Type> {
    match data {
        syn::Data::Struct(data) => data.fields.iter().map(|f| f.ty.clone()).collect(),
        syn::Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|v| v.fields.iter().map(|f| f.ty.clone()))
            .collect(),
        syn::Data::Union(_) => unreachable!(),
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

fn gen_erased_family_impls(owner_name: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let name = gen_erased_name(owner_name);
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
