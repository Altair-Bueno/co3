use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, visit::Visit};

use crate::layout::{
    attr::ReprKind, enum_tag_type, is_transparent_enum_repr, is_type_parameterized,
};

fn lowered_field_ty(field_ty: &syn::Type) -> TokenStream {
    quote!(<#field_ty as co3::ExternC>::CType)
}

pub(super) fn gen_item_ctype(repr: Option<&ReprKind>, input: &syn::DeriveInput) -> TokenStream {
    let vis = &input.vis;
    let name = &input.ident;
    let generics = &input.generics;

    match &input.data {
        syn::Data::Struct(data) => {
            derive_ctype_struct::<false>(repr, vis, name, generics, &data.fields)
        }
        syn::Data::Enum(data) if is_transparent_enum_repr(repr, &data.variants) => {
            let Some(first_variant) = data.variants.first() else {
                return quote! {};
            };

            derive_ctype_struct::<true>(repr, vis, name, generics, &first_variant.fields)
        }
        syn::Data::Enum(data) if matches!(repr, Some(&ReprKind::Primitive(_)) | None) => {
            let tag_type = enum_tag_type(repr, data.variants.len()).unwrap();
            derive_data_enum_ctype(tag_type, vis, name, generics, &data.variants)
        }
        syn::Data::Enum(data) if matches!(repr, Some(&ReprKind::C(Some(_)))) => {
            let tag_type = enum_tag_type(repr, data.variants.len()).unwrap();
            derive_repr_c_data_enum_ctype(tag_type, vis, name, generics, &data.variants)
        }
        syn::Data::Union(_) | syn::Data::Enum(_) => {
            unreachable!()
        }
    }
}

fn derive_ctype_struct<const ADD_COPY: bool>(
    repr: Option<&ReprKind>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
) -> TokenStream {
    let ctype_def = gen_ctype_struct_item::<ADD_COPY>(repr, vis, name, generics, fields);

    let ctype_impls = gen_struct_ctype_impls::<ADD_COPY>(&ctype_def);

    quote! {
        #ctype_def
        #ctype_impls
    }
}

fn derive_data_enum_ctype(
    tag_type: syn::Type,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let union_name = gen_ctype_name(name);

    let (union_def, variant_structs) =
        gen_data_enum_union(Some(tag_type), vis, &union_name, name, generics, variants);

    let union_impls = gen_union_ctype_impls(&union_def);

    quote! {
        #(#variant_structs)*

        #union_def
        #union_impls
    }
}

fn derive_repr_c_data_enum_ctype(
    tag_type: syn::Type,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let payload_name = format_ident!("{name}Payload");

    let (payload_def, variant_structs) = gen_data_enum_union(
        None,
        &syn::Visibility::Inherited,
        &payload_name,
        name,
        generics,
        variants,
    );

    let payload_impls = gen_union_ctype_impls(&payload_def);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let ctype_name = gen_ctype_name(name);
    let ctype_bounds = gen_union_extern_c_bounds(generics, variants);

    let ctype_def: syn::ItemStruct = parse_quote! {
        #[repr(C)]
        #[doc(hidden)]
        #vis struct #ctype_name #impl_generics
        where
            #(#ctype_bounds,)*
            #predicates
        {
            tag: #tag_type,
            payload: #payload_name #ty_generics,
        }
    };

    let ctype_impls = gen_struct_ctype_impls::<true>(&ctype_def);

    quote! {
        #(#variant_structs)*

        #payload_def
        #payload_impls

        #ctype_def
        #ctype_impls
    }
}

fn gen_ctype_struct_item<const ADD_COPY: bool>(
    repr: Option<&ReprKind>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
) -> syn::ItemStruct {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let ctype_name = gen_ctype_name(name);
    let repr = gen_ctype_repr_attr(repr);

    let field_types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let extern_c_bounds = gen_extern_c_bounds_for_ctype::<ADD_COPY>(generics, &field_types);

    let field_c_tys = fields
        .iter()
        .map(|field| lowered_field_ty(&field.ty))
        .collect::<Vec<_>>();

    let ctype = match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => {
            let field_names = fields.iter().map(|field| &field.ident);

            quote! {
                struct #ctype_name #impl_generics
                where
                    #(#extern_c_bounds,)*
                    #predicates
                {
                    #(#field_names: #field_c_tys),*
                }
            }
        }
        syn::Fields::Unnamed(_) => quote! {
            struct #ctype_name #impl_generics (#(#field_c_tys),*)
            where
                #(#extern_c_bounds,)*
                #predicates;
        },
    };

    parse_quote! {
        #repr
        #[derive(co3::rust_spec::RustSpec)]
        #[doc(hidden)]
        #vis #ctype
    }
}

fn gen_data_enum_union(
    variant_tag: Option<syn::Type>,
    vis: &syn::Visibility,
    union_name: &syn::Ident,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> (syn::ItemUnion, Vec<TokenStream>) {
    let (variant_generics, union_fields) = gen_union_fields(enum_name, generics, variants);

    let variant_structs = variants
        .iter()
        .zip(&variant_generics)
        .map(|(variant, generics)| {
            gen_variant_struct(variant_tag.as_ref(), enum_name, generics, variant)
        });

    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let union_extern_c_bounds = gen_union_extern_c_bounds(generics, variants);

    let union_def = parse_quote! {
        #[repr(C)]
        #[doc(hidden)]
        #[expect(non_snake_case)]
        #vis union #union_name #impl_generics
        where
            #(#union_extern_c_bounds,)*
            #predicates
        #union_fields
    };

    (union_def, variant_structs.collect())
}

fn gen_variant_struct(
    tag_type: Option<&syn::Type>,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variant: &syn::Variant,
) -> TokenStream {
    let name = format_ident!("{enum_name}{}", &variant.ident);

    let repr = Some(&ReprKind::C(None));
    let vis = syn::Visibility::Inherited;
    let mut fields = variant.fields.clone();

    if let Some(tag_type) = tag_type {
        match &mut fields {
            syn::Fields::Unit => {
                fields = syn::Fields::Named(parse_quote!({ tag: #tag_type }));
            }
            syn::Fields::Named(fields) => {
                fields.named.insert(0, parse_quote!(tag: #tag_type));
            }
            syn::Fields::Unnamed(fields) => {
                fields.unnamed.insert(0, parse_quote!(#tag_type));
            }
        }
    }

    let ctype = gen_ctype_struct_item::<true>(repr, &vis, &name, generics, &fields);
    let ctype_impls = gen_struct_ctype_impls::<true>(&ctype);
    quote! { #ctype #ctype_impls }
}

fn gen_union_fields(
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> (Vec<syn::Generics>, syn::FieldsNamed) {
    let filtered_generics = variants
        .iter()
        .map(|variant| {
            let variant_fields = variant.fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
            filter_generics(generics, &variant_fields)
        })
        .collect::<Vec<_>>();

    let named =
        variants
            .iter()
            .zip(&filtered_generics)
            .map(|(variant, variant_generics)| -> syn::Field {
                let variant_name = &variant.ident;
                let (_, ty_generics, _) = variant_generics.split_for_impl();
                let variant_struct_name = gen_variant_struct_name(name, variant_name);
                parse_quote!(#variant_name: #variant_struct_name #ty_generics)
            });

    let fields = syn::FieldsNamed {
        brace_token: Default::default(),
        named: named.collect(),
    };

    (filtered_generics, fields)
}

fn gen_union_extern_c_bounds(
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> impl Iterator<Item = syn::WherePredicate> {
    let mut seen = HashSet::new();

    let unique_types = variants
        .iter()
        .flat_map(|v| v.fields.iter().map(|f| &f.ty))
        .filter(|&ty| seen.insert(ty))
        .collect::<Vec<_>>();

    gen_extern_c_bounds_for_ctype::<true>(generics, &unique_types)
        .into_iter()
        .map(|p| {
            let predicate: syn::WherePredicate = parse_quote!(#p);
            predicate
        })
}

fn gen_struct_ctype_impls<const ADD_COPY: bool>(ctype: &syn::ItemStruct) -> TokenStream {
    let fields = ctype.fields.iter().map(|f| &f.ty).collect::<Vec<_>>();

    let copy_impls = gen_copy_impls::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);
    let default_impl = gen_default_impl::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);
    let robust_impls = gen_robust_impls::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);

    let const_view = gen_ctype_struct_view::<ADD_COPY>(ctype.clone(), false);
    let mut_view = gen_ctype_struct_view::<ADD_COPY>(ctype.clone(), true);

    let borrow_cast_impl = gen_borrow_cast_impl::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);

    quote! {
        #copy_impls
        #default_impl
        #robust_impls

        #const_view
        #mut_view

        #borrow_cast_impl
    }
}

fn gen_union_ctype_impls(ctype: &syn::ItemUnion) -> TokenStream {
    let fields = ctype.fields.named.iter().map(|f| &f.ty).collect::<Vec<_>>();

    let copy_impls = gen_copy_impls::<true>(&ctype.ident, &ctype.generics, &fields);
    let default_impl = gen_default_impl::<true>(&ctype.ident, &ctype.generics, &fields);
    let robust_impls = gen_robust_impls::<true>(&ctype.ident, &ctype.generics, &fields);
    let type_spec_impl = gen_repr_c_type_spec_impl(&ctype.ident, &ctype.generics);

    let const_view = gen_ctype_union_view(ctype.clone(), false);
    let mut_view = gen_ctype_union_view(ctype.clone(), true);

    let borrow_cast_impl = gen_borrow_cast_impl::<true>(&ctype.ident, &ctype.generics, &fields);

    quote! {
        #copy_impls
        #default_impl
        #robust_impls
        #type_spec_impl

        #const_view
        #mut_view

        #borrow_cast_impl
    }
}

fn gen_robust_impls<const ADD_COPY: bool>(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let copy_bounds = gen_copy_bounds::<ADD_COPY>(generics, fields);
    let codec_impls = gen_identity_codec_impls::<ADD_COPY>(ident, generics, fields);

    let for_dummy = (generics.type_params().count() == 0).then_some(quote! { for<'_dummy> });

    let type_spec_bound = (!ADD_COPY).then(|| {
        quote! { #for_dummy Self: co3::rust_spec::RustSpec<Size = co3::rust_spec::size::Sized<co3::rust_spec::size::NonZst>>, }
    });

    quote! {
        unsafe impl #impl_generics co3::ReprC for #ident #ty_generics #where_clause {}

        unsafe impl #impl_generics co3::CFnArg for #ident #ty_generics
        where
            #type_spec_bound
            #(#copy_bounds,)*
            #predicates
        {}

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #ident #ty_generics #where_clause {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        #codec_impls
    }
}

fn gen_repr_c_type_spec_impl(ident: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        unsafe impl #impl_generics co3::rust_spec::RustSpec for #ident #ty_generics #where_clause {
            type Layout = co3::rust_spec::layout::Stable<co3::rust_spec::layout::Robust>;
            type Size = co3::rust_spec::size::Sized<co3::rust_spec::size::NonZst>;
            type Niche = co3::rust_spec::niche::WithoutNiche;
            type Mutability = co3::rust_spec::mutability::Exclusive;
            type __IndirectLayout = co3::rust_spec::layout::Stable<co3::rust_spec::layout::Robust>;
        }
    }
}

fn gen_identity_codec_impls<const ADD_COPY: bool>(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let params = &generics.params;

    let copy_bounds = gen_copy_bounds::<ADD_COPY>(generics, fields);

    quote! {
        impl #impl_generics co3::ExternC for #ident #ty_generics
        where
            #predicates
        {
            type CType = Self;
        }
        unsafe impl #impl_generics co3::stored::EncodeOwned for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl<'d, #params> co3::stored::DecodeOwned<'d> for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl #impl_generics co3::Encode for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {}
        impl #impl_generics co3::Decode<'_> for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {}
    }
}

fn gen_borrow_cast_impl<const ADD_COPY: bool>(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let const_view_name = gen_const_view_name(ident);
    let mut_view_name = gen_mut_view_name(ident);

    let const_bounds = gen_ctype_borrow_cast_bounds::<ADD_COPY>(generics, fields, false);
    let mut_bounds = gen_ctype_borrow_cast_bounds::<ADD_COPY>(generics, fields, true);

    quote! {
        unsafe impl #impl_generics co3::borrow::BorrowCast for #ident #ty_generics
        where
            #(#const_bounds,)*
            #predicates
        {
            type AsConst = #const_view_name #ty_generics;
        }

        unsafe impl #impl_generics co3::borrow::BorrowCastMut for #ident #ty_generics
        where
            #(#mut_bounds,)*
            #predicates
        {
            type AsMut = #mut_view_name #ty_generics;
        }
    }
}

fn gen_ctype_repr_attr(repr: Option<&ReprKind>) -> TokenStream {
    if repr == Some(&ReprKind::Transparent) {
        quote! { #[repr(transparent)] }
    } else {
        quote! { #[repr(C)] }
    }
}

fn gen_ctype_struct_view<const ADD_COPY: bool>(
    mut ctype: syn::ItemStruct,
    is_mut: bool,
) -> TokenStream {
    rewrite_ctype_view_struct::<ADD_COPY>(&mut ctype, is_mut);

    let fields = ctype
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let copy_impls = gen_copy_impls::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);
    let default_impl = gen_default_impl::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);
    let robust_impls = gen_robust_impls::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);

    quote! {
        #ctype
        #copy_impls
        #default_impl
        #robust_impls
    }
}

fn gen_ctype_union_view(mut ctype: syn::ItemUnion, is_mut: bool) -> TokenStream {
    rewrite_ctype_view_union(&mut ctype, is_mut);

    let fields = ctype
        .fields
        .named
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let copy_impls = gen_copy_impls::<true>(&ctype.ident, &ctype.generics, &fields);
    let default_impl = gen_default_impl::<true>(&ctype.ident, &ctype.generics, &fields);
    let robust_impls = gen_robust_impls::<true>(&ctype.ident, &ctype.generics, &fields);
    let type_spec_impl = gen_repr_c_type_spec_impl(&ctype.ident, &ctype.generics);

    quote! {
        #ctype
        #copy_impls
        #default_impl
        #robust_impls
        #type_spec_impl
    }
}

fn rewrite_ctype_view_struct<const ADD_COPY: bool>(ctype: &mut syn::ItemStruct, is_mut: bool) {
    rewrite_ctype_view_name(&mut ctype.ident, is_mut);
    rewrite_ctype_view_generics::<ADD_COPY>(
        &mut ctype.generics,
        ctype.fields.iter_mut().map(|field| &mut field.ty),
        is_mut,
    );
}

fn rewrite_ctype_view_union(ctype: &mut syn::ItemUnion, is_mut: bool) {
    rewrite_ctype_view_name(&mut ctype.ident, is_mut);
    rewrite_ctype_view_generics::<true>(
        &mut ctype.generics,
        ctype.fields.named.iter_mut().map(|field| &mut field.ty),
        is_mut,
    );
}

fn rewrite_ctype_view_name(ident: &mut syn::Ident, is_mut: bool) {
    *ident = if is_mut {
        gen_mut_view_name(ident)
    } else {
        gen_const_view_name(ident)
    };
}

fn rewrite_ctype_view_generics<'a, const ADD_COPY: bool>(
    generics: &mut syn::Generics,
    fields: impl Iterator<Item = &'a mut syn::Type>,
    is_mut: bool,
) {
    let field_tys = rewrite_ctype_view_field_tys(fields, is_mut);
    let field_tys = field_tys.iter().collect::<Vec<_>>();

    for bound in gen_ctype_borrow_cast_bounds::<ADD_COPY>(generics, &field_tys, is_mut) {
        generics.make_where_clause().predicates.push(parse_quote! {
            #bound
        });
    }
}

fn rewrite_ctype_view_field_tys<'a>(
    fields: impl Iterator<Item = &'a mut syn::Type>,
    is_mut: bool,
) -> Vec<syn::Type> {
    fields
        .map(|field_ty| {
            let ty = field_ty.clone();
            let (borrow_cast_trait, view_ty) = if is_mut {
                (quote!(co3::borrow::BorrowCastMut), quote!(AsMut))
            } else {
                (quote!(co3::borrow::BorrowCast), quote!(AsConst))
            };
            *field_ty = parse_quote! { <#ty as #borrow_cast_trait>::#view_ty };
            ty
        })
        .collect()
}

fn gen_copy_impls<const ADD_COPY: bool>(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let copy_bounds = gen_copy_bounds::<ADD_COPY>(generics, fields);

    quote! {
        impl #impl_generics Clone for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {
            fn clone(&self) -> Self {
                *self
            }
        }

        impl #impl_generics Copy for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {}
    }
}

fn gen_copy_bounds<const ADD_COPY: bool>(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<TokenStream> {
    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    let mut predicates = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|&ty| parse_quote! { #ty: Copy })
        .collect::<Vec<_>>();

    if is_type_parameterized(last, generics) {
        predicates.push(quote!(#last: Copy));
    } else if !ADD_COPY {
        predicates.push(quote!(for<'_dummy> #last: Copy));
    }

    predicates
}

fn gen_default_impl<const ADD_COPY: bool>(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let copy_bounds = gen_copy_bounds::<ADD_COPY>(generics, fields);

    quote! {
        impl #impl_generics Default for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {
            #[inline(always)]
            fn default() -> Self {
                unsafe { core::mem::zeroed() }
            }
        }
    }
}

pub(super) fn gen_ctype_name(item_name: &syn::Ident) -> syn::Ident {
    format_ident!("C{item_name}")
}

fn gen_const_view_name(item_name: &syn::Ident) -> syn::Ident {
    format_ident!("{item_name}ConstView")
}

fn gen_mut_view_name(item_name: &syn::Ident) -> syn::Ident {
    format_ident!("{item_name}MutView")
}

pub(super) fn gen_variant_struct_name(
    enum_name: &syn::Ident,
    variant_name: &syn::Ident,
) -> syn::Ident {
    format_ident!("C{enum_name}{variant_name}")
}

struct UsedGenericsVisitor<'a> {
    generics: &'a syn::Generics,
    used_lifetimes: std::collections::HashSet<&'a syn::Ident>,
    used_type_params: std::collections::HashSet<&'a syn::Ident>,
    used_const_params: std::collections::HashSet<&'a syn::Ident>,
}

impl<'a> UsedGenericsVisitor<'a> {
    fn new(generics: &'a syn::Generics) -> Self {
        Self {
            generics,
            used_lifetimes: std::collections::HashSet::new(),
            used_type_params: std::collections::HashSet::new(),
            used_const_params: std::collections::HashSet::new(),
        }
    }
}

impl<'a> Visit<'_> for UsedGenericsVisitor<'a> {
    fn visit_lifetime(&mut self, lifetime: &syn::Lifetime) {
        for lt in self.generics.lifetimes() {
            if lt.lifetime.ident == lifetime.ident {
                self.used_lifetimes.insert(&lt.lifetime.ident);
            }
        }

        syn::visit::visit_lifetime(self, lifetime);
    }

    fn visit_type_path(&mut self, type_path: &syn::TypePath) {
        if let Some(ident) = type_path.path.get_ident() {
            for tp in self.generics.type_params() {
                if &tp.ident == ident {
                    self.used_type_params.insert(&tp.ident);
                }
            }
            for cp in self.generics.const_params() {
                if &cp.ident == ident {
                    self.used_const_params.insert(&cp.ident);
                }
            }
        }

        syn::visit::visit_type_path(self, type_path);
    }

    fn visit_expr_path(&mut self, expr_path: &syn::ExprPath) {
        if let Some(ident) = expr_path.path.get_ident() {
            for cp in self.generics.const_params() {
                if &cp.ident == ident {
                    self.used_const_params.insert(&cp.ident);
                }
            }
        }

        syn::visit::visit_expr_path(self, expr_path);
    }
}

pub(super) fn filter_generics(
    generics: &syn::Generics,
    field_types: &[&syn::Type],
) -> syn::Generics {
    let mut visitor = UsedGenericsVisitor::new(generics);

    for ty in field_types {
        visitor.visit_type(ty);
    }

    let mut filtered = syn::Generics {
        params: generics
            .params
            .iter()
            .filter(|param| match param {
                syn::GenericParam::Lifetime(lt) => {
                    visitor.used_lifetimes.contains(&lt.lifetime.ident)
                }
                syn::GenericParam::Type(tp) => visitor.used_type_params.contains(&tp.ident),
                syn::GenericParam::Const(cp) => visitor.used_const_params.contains(&cp.ident),
            })
            .cloned()
            .collect(),
        ..Default::default()
    };

    if let Some(where_clause) = &generics.where_clause {
        let retained_lifetimes = filtered
            .lifetimes()
            .map(|lt| lt.lifetime.ident.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let retained_type_params = filtered
            .type_params()
            .map(|tp| tp.ident.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let retained_const_params = filtered
            .const_params()
            .map(|cp| cp.ident.clone())
            .collect::<std::collections::BTreeSet<_>>();

        let predicates = where_clause
            .predicates
            .iter()
            .filter(|predicate| {
                let mut predicate_visitor = UsedGenericsVisitor::new(generics);
                predicate_visitor.visit_where_predicate(predicate);

                predicate_visitor
                    .used_lifetimes
                    .into_iter()
                    .all(|ident| retained_lifetimes.contains(ident))
                    && predicate_visitor
                        .used_type_params
                        .into_iter()
                        .all(|ident| retained_type_params.contains(ident))
                    && predicate_visitor
                        .used_const_params
                        .into_iter()
                        .all(|ident| retained_const_params.contains(ident))
            })
            .cloned()
            .collect::<syn::punctuated::Punctuated<_, syn::token::Comma>>();

        filtered.where_clause = (!predicates.is_empty()).then_some(syn::WhereClause {
            where_token: where_clause.where_token,
            predicates,
        });
    }

    filtered
}

pub(super) fn gen_extern_c_bounds_for_ctype<const ADD_COPY: bool>(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<TokenStream> {
    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    let mut predicates = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|&ty| {
            let ctype_bound = if ADD_COPY {
                quote! { <CType: Copy> }
            } else {
                quote! { <CType: Sized> }
            };

            quote! { #ty: co3::ExternC #ctype_bound }
        })
        .collect::<Vec<_>>();

    if is_type_parameterized(last, generics) {
        let copy_bound = ADD_COPY.then(|| quote! { <CType: Copy> });
        predicates.push(quote! { #last: co3::ExternC #copy_bound });
    }

    predicates
}

fn gen_ctype_borrow_cast_bounds<const ADD_COPY: bool>(
    generics: &syn::Generics,
    fields: &[&syn::Type],
    is_mut: bool,
) -> Vec<TokenStream> {
    let (borrow_cast_trait, assoc_type) = if is_mut {
        (quote! { co3::borrow::BorrowCastMut }, quote!(AsMut))
    } else {
        (quote! { co3::borrow::BorrowCast }, quote!(AsConst))
    };

    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    let mut predicates = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|&ty| {
            let ctype_bound = if ADD_COPY {
                quote! { <#assoc_type: Copy> }
            } else {
                quote! { <#assoc_type: Sized> }
            };

            parse_quote! { #ty: #borrow_cast_trait #ctype_bound }
        })
        .collect::<Vec<_>>();

    if is_type_parameterized(last, generics) {
        let copy_bound = ADD_COPY.then(|| quote! { <#assoc_type: Copy> });
        predicates.push(parse_quote! { #last: #borrow_cast_trait #copy_bound });
    }

    predicates
}
