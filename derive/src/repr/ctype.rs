use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Visibility, parse_quote, visit::Visit};

use crate::repr::{
    attr::ReprKind, enum_tag_type, gen_size_family_impl, gen_sized_family_impl,
    is_type_parameterized,
};

fn lowered_field_ty(field_ty: &syn::Type) -> TokenStream {
    quote!(<#field_ty as co3::ExternC>::CType)
}

pub(super) fn gen_item_ctype(repr: Option<&ReprKind>, input: &syn::DeriveInput) -> TokenStream {
    let vis = &input.vis;
    let name = &input.ident;
    let generics = &input.generics;

    match &input.data {
        syn::Data::Struct(data) => derive_ctype_struct(repr, vis, name, generics, &data.fields),
        syn::Data::Enum(data) if matches!(repr, Some(&ReprKind::Transparent)) => {
            let Some(first_variant) = data.variants.first() else {
                return quote! {};
            };

            derive_ctype_struct(repr, vis, name, generics, &first_variant.fields)
        }
        syn::Data::Enum(data) if matches!(repr, Some(&ReprKind::Primitive(_))) => {
            derive_data_enum_ctype(repr, vis, name, generics, &data.variants)
        }
        syn::Data::Enum(data) if matches!(repr, Some(&ReprKind::C(Some(_)))) => {
            derive_repr_c_data_enum_ctype(repr, vis, name, generics, &data.variants)
        }
        syn::Data::Union(_) | syn::Data::Enum(_) => {
            unreachable!()
        }
    }
}

fn derive_ctype_struct(
    repr: Option<&ReprKind>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
) -> TokenStream {
    let ctype_def = gen_ctype_struct_item(repr, vis, name, generics, fields);

    let ctype_impls = gen_struct_ctype_impls(&ctype_def);
    let ctype_last_field_assert = assert_ctype_last_field_copy_or_unsized(
        &ctype_def.ident,
        &ctype_def.generics,
        &ctype_def.fields,
    );

    let fields = ctype_def.fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let borrow_cast_impl = gen_borrow_cast_impl(name, &ctype_def.generics, &fields);

    quote! {
        #ctype_last_field_assert

        #ctype_def
        #ctype_impls
        #borrow_cast_impl
    }
}

fn derive_data_enum_ctype(
    repr: Option<&ReprKind>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let variant_structs = variants
        .iter()
        .map(|variant| gen_variant_struct(repr, name, generics, variant));

    let union_fields = {
        let named = variants.iter().map(|variant| -> syn::Field {
            let variant_name = &variant.ident;

            let variant_fields = variant
                .fields
                .iter()
                .map(|field| &field.ty)
                .collect::<Vec<_>>();

            let filtered_generics = filter_generics(&variant_fields, generics);
            let (_, ty_generics, _) = filtered_generics.split_for_impl();

            let variant_struct_name = gen_variant_struct_name(name, variant_name);
            parse_quote! { #variant_name: #variant_struct_name #ty_generics }
        });

        syn::FieldsNamed {
            brace_token: Default::default(),
            named: named.collect(),
        }
    };

    let ctype_def = gen_ctype_union(repr, vis, name, generics, &union_fields);
    let union_impls = gen_union_ctype_impls(&ctype_def);

    let fields = ctype_def
        .fields
        .named
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let borrow_cast_impl = gen_borrow_cast_impl(name, &ctype_def.generics, &fields);

    quote! {
        #(#variant_structs)*
        #ctype_def
        #union_impls
        #borrow_cast_impl
    }
}

fn derive_repr_c_data_enum_ctype(
    repr: Option<&ReprKind>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let union_fields = {
        let named = variants.iter().map(|variant| -> syn::Field {
            let variant_name = &variant.ident;

            let variant_fields = variant
                .fields
                .iter()
                .map(|field| &field.ty)
                .collect::<Vec<_>>();

            let filtered_generics = filter_generics(&variant_fields, generics);
            let (_, ty_generics, _) = filtered_generics.split_for_impl();

            let variant_struct_name = gen_variant_struct_name(name, variant_name);
            parse_quote! { #variant_name: #variant_struct_name #ty_generics }
        });

        syn::FieldsNamed {
            brace_token: Default::default(),
            named: named.collect(),
        }
    };

    let payload_vis = syn::Visibility::Inherited;
    let payload_name = format_ident!("{name}Payload");
    let payload_def = gen_ctype_union(repr, &payload_vis, &payload_name, generics, &union_fields);
    let payload_impls = gen_union_ctype_impls(&payload_def);

    let tag_type = enum_tag_type(repr, variants.len()).unwrap();
    let payload_ctype_name = gen_ctype_name(&payload_name);
    let payload_ty_generics = generic_args(generics);

    let fields = syn::Fields::Named(parse_quote!({
        tag: #tag_type,
        payload: #payload_ctype_name #payload_ty_generics
    }));

    let ctype_def = gen_ctype_struct_item(Some(&ReprKind::C(None)), vis, name, generics, &fields);
    let ctype_impls = gen_struct_ctype_impls(&ctype_def);

    let fields = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let borrow_cast_impl = gen_borrow_cast_impl(name, &ctype_def.generics, &fields);

    quote! {
        #payload_def
        #payload_impls

        #ctype_def
        #ctype_impls

        #borrow_cast_impl
    }
}

fn gen_ctype_struct_item(
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
    let extern_c_bounds = gen_extern_c_bounds_for_ctype::<false>(generics, &field_types);

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
                    #extern_c_bounds
                    #predicates
                {
                    #(#field_names: #field_c_tys),*
                }
            }
        }
        syn::Fields::Unnamed(_) => quote! {
            struct #ctype_name #impl_generics (#(#field_c_tys),*)
            where
                #extern_c_bounds
                #predicates;
        },
    };

    parse_quote! {
        #repr
        #[doc(hidden)]
        #vis #ctype
    }
}

fn gen_ctype_union(
    repr: Option<&ReprKind>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::FieldsNamed,
) -> syn::ItemUnion {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let c_type_name = gen_ctype_name(name);
    let repr = gen_ctype_repr_attr(repr);

    let field_types = fields.named.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let extern_c_bounds = gen_extern_c_bounds_for_ctype::<true>(generics, &field_types);

    let field_names = fields
        .named
        .iter()
        .map(|variant| &variant.ident)
        .collect::<Vec<_>>();

    let field_c_tys = fields
        .named
        .iter()
        .map(|field| lowered_field_ty(&field.ty))
        .collect::<Vec<_>>();

    parse_quote! {
        #repr
        #[doc(hidden)]
        #[expect(non_snake_case)]
        #vis union #c_type_name #impl_generics
        where
            #extern_c_bounds
            #predicates
        {
            #(#field_names: #field_c_tys),*
        }
    }
}

fn gen_variant_struct(
    repr: Option<&ReprKind>,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variant: &syn::Variant,
) -> TokenStream {
    let vis = Visibility::Inherited;

    let variant_fields = variant
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let name = format_ident!("{enum_name}V{}", &variant.ident);
    let filtered_generics = filter_generics(&variant_fields, generics);

    let ctype_def = gen_ctype_struct_item(repr, &vis, &name, &filtered_generics, &variant.fields);
    let ctype_impls = gen_struct_ctype_impls(&ctype_def);

    quote! {
        #ctype_def
        #ctype_impls
    }
}

fn gen_struct_ctype_impls(ctype: &syn::ItemStruct) -> TokenStream {
    let fields = ctype
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let copy_impls = gen_copy_impls(&ctype.ident, &ctype.generics, &fields);
    let default_impl = gen_default_impl(&ctype.ident, &ctype.generics);
    let robust_impls = gen_robust_impls(&ctype.ident, &ctype.generics, &fields);
    let size_family_impl = gen_size_family_impl(&ctype.ident, &ctype.generics, &fields);

    let const_view = gen_ctype_struct_view(ctype.clone(), false);
    let mut_view = gen_ctype_struct_view(ctype.clone(), true);

    quote! {
        #copy_impls
        #default_impl
        #robust_impls
        #size_family_impl

        #const_view
        #mut_view
    }
}

fn gen_union_ctype_impls(ctype: &syn::ItemUnion) -> TokenStream {
    let field_tys = ctype
        .fields
        .named
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let copy_impls = gen_copy_impls(&ctype.ident, &ctype.generics, &field_tys);
    let default_impl = gen_default_impl(&ctype.ident, &ctype.generics);
    let robust_impls = gen_robust_impls(&ctype.ident, &ctype.generics, &field_tys);
    let size_family_impl = gen_sized_family_impl(&ctype.ident, &ctype.generics);

    let const_view = gen_ctype_union_view(ctype.clone(), false);
    let mut_view = gen_ctype_union_view(ctype.clone(), true);

    quote! {
        #copy_impls
        #default_impl
        #robust_impls
        #size_family_impl

        #const_view
        #mut_view
    }
}

fn gen_robust_impls(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let copy_bounds = gen_last_field_copy_bound(generics, fields);
    let codec_impls = gen_identity_codec_impls(ident, generics, fields);

    quote! {
        impl #impl_generics co3::ir::ReprFamily for #ident #ty_generics #where_clause {
            type Kind = co3::ir::Transmuted<co3::ir::Robust>;
        }

        impl #impl_generics co3::niche::NicheFamily for #ident #ty_generics where
            #copy_bounds
            #predicates
        {
            type Kind = co3::niche::WithoutNiche;
        }

        unsafe impl #impl_generics co3::ReprC for #ident #ty_generics
        where
            #predicates
        {}
        unsafe impl #impl_generics co3::CFnArg for #ident #ty_generics
        where
            #copy_bounds
            #predicates
        {}

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #ident #ty_generics
        where
            #copy_bounds
            #predicates
        {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        #codec_impls

        unsafe impl #impl_generics co3::handle::Erase for #ident #ty_generics #where_clause {
            type Erased = Self;
        }
    }
}

fn gen_identity_codec_impls(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let params = &generics.params;

    let copy_bounds = gen_last_field_copy_bound(generics, fields);

    quote! {
        impl #impl_generics co3::ExternC for #ident #ty_generics
        where
            #predicates
        {
            type CType = Self;
        }
        impl #impl_generics co3::stored::SoftEncodeOwned for #ident #ty_generics
        where
            #copy_bounds
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
        impl<'d, #params> co3::stored::SoftDecodeOwned<'d> for #ident #ty_generics
        where
            #copy_bounds
            #predicates
        {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl #impl_generics co3::SoftEncode for #ident #ty_generics
        where
            #copy_bounds
            #predicates
        {}
        impl #impl_generics co3::SoftDecode<'_> for #ident #ty_generics
        where
            #copy_bounds
            #predicates
        {}
    }
}

fn gen_borrow_cast_impl(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let ctype_name = gen_ctype_name(ident);
    let const_view_name = gen_const_view_name(&ctype_name);
    let mut_view_name = gen_mut_view_name(&ctype_name);
    let copy_bounds = gen_last_field_copy_bound(generics, fields);

    let copy_bound = quote!(<AsConst: Copy, AsMut: Copy>);
    let borrow_cast_bounds = gen_ctype_borrow_cast_bounds(generics, fields, copy_bound);

    quote! {
        unsafe impl #impl_generics co3::borrow::BorrowCast for #ctype_name #ty_generics
        where
            #(#borrow_cast_bounds,)*
            #copy_bounds
            #predicates
        {
            type AsConst = #const_view_name #ty_generics;
            type AsMut = #mut_view_name #ty_generics;
        }
    }
}

fn generic_args(generics: &syn::Generics) -> TokenStream {
    let args = generics.params.iter().map(|param| match param {
        syn::GenericParam::Lifetime(param) => {
            let lifetime = &param.lifetime;
            quote! { #lifetime }
        }
        syn::GenericParam::Type(param) => {
            let ident = &param.ident;
            quote! { #ident }
        }
        syn::GenericParam::Const(param) => {
            let ident = &param.ident;
            quote! { #ident }
        }
    });

    quote! { <#(#args),*> }
}

fn gen_ctype_repr_attr(repr: Option<&ReprKind>) -> TokenStream {
    // TODO: We could use #[repr(transparent)]. Check:
    // https://github.com/rust-lang/rust/issues/60405
    if repr == Some(&ReprKind::Transparent) {
        quote! { #[repr(transparent)] }
    } else {
        quote! { #[repr(C)] }
    }
}

fn gen_ctype_struct_view(mut ctype: syn::ItemStruct, is_mut: bool) -> TokenStream {
    rewrite_ctype_view_struct(&mut ctype, is_mut);

    let fields = ctype
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    let copy_impls = gen_copy_impls(&ctype.ident, &ctype.generics, &fields);
    let default_impl = gen_default_impl(&ctype.ident, &ctype.generics);
    let robust_impls = gen_robust_impls(&ctype.ident, &ctype.generics, &fields);
    let size_family_impl = gen_size_family_impl(&ctype.ident, &ctype.generics, &fields);

    quote! {
        #ctype
        #copy_impls
        #default_impl
        #robust_impls
        #size_family_impl
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

    let copy_impls = gen_copy_impls(&ctype.ident, &ctype.generics, &fields);
    let default_impl = gen_default_impl(&ctype.ident, &ctype.generics);
    let robust_impls = gen_robust_impls(&ctype.ident, &ctype.generics, &fields);
    let size_family_impl = gen_sized_family_impl(&ctype.ident, &ctype.generics);

    quote! {
        #ctype
        #copy_impls
        #default_impl
        #robust_impls
        #size_family_impl
    }
}

fn rewrite_ctype_view_struct(ctype: &mut syn::ItemStruct, is_mut: bool) {
    rewrite_ctype_view_name(&mut ctype.ident, is_mut);
    rewrite_ctype_view_generics(
        &mut ctype.generics,
        ctype.fields.iter_mut().map(|field| &mut field.ty),
        is_mut,
    );
}

fn rewrite_ctype_view_union(ctype: &mut syn::ItemUnion, is_mut: bool) {
    rewrite_ctype_view_name(&mut ctype.ident, is_mut);
    rewrite_ctype_view_generics(
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

fn rewrite_ctype_view_generics<'a>(
    generics: &mut syn::Generics,
    fields: impl Iterator<Item = &'a mut syn::Type>,
    is_mut: bool,
) {
    let field_tys = rewrite_ctype_view_field_tys(fields, is_mut);
    let field_tys = field_tys.iter().collect::<Vec<_>>();

    let copy_bound = if is_mut {
        quote! { <AsMut: Copy> }
    } else {
        quote! { <AsConst: Copy> }
    };

    let borrow_cast_bounds = gen_ctype_borrow_cast_bounds(generics, &field_tys, copy_bound);

    let where_clause = generics.make_where_clause();
    where_clause.predicates.extend(borrow_cast_bounds);
}

fn rewrite_ctype_view_field_tys<'a>(
    fields: impl Iterator<Item = &'a mut syn::Type>,
    is_mut: bool,
) -> Vec<syn::Type> {
    fields
        .map(|field_ty| {
            let ty = field_ty.clone();
            let view_ty = if is_mut {
                quote!(AsMut)
            } else {
                quote!(AsConst)
            };
            *field_ty = parse_quote! { <#ty as co3::borrow::BorrowCast>::#view_ty };
            ty
        })
        .collect()
}

pub(crate) fn gen_ctype_borrow_cast_bounds(
    generics: &syn::Generics,
    fields: &[&syn::Type],
    copy_bound: TokenStream,
) -> Vec<syn::WherePredicate> {
    let mut predicates = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|ty| parse_quote! { #ty: co3::borrow::BorrowCast #copy_bound })
        .collect::<Vec<syn::WherePredicate>>();

    if let Some(last) = fields.last()
        && !is_type_parameterized(last, generics)
    {
        predicates.push(parse_quote!(for<'_dummy> #last: co3::borrow::BorrowCast #copy_bound));
    }

    predicates
}

fn gen_copy_impls(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let copy_bounds = gen_last_field_copy_bound(generics, fields);

    quote! {
        impl #impl_generics Clone for #ident #ty_generics
        where
            #copy_bounds
            #predicates
        {
            fn clone(&self) -> Self {
                *self
            }
        }

        impl #impl_generics Copy for #ident #ty_generics
        where
            #copy_bounds
            #predicates
        {}
    }
}

fn assert_ctype_last_field_copy_or_unsized(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let Some(last) = fields.iter().last().map(|f| &f.ty) else {
        return quote!();
    };

    quote! {
        const _: () = {
            #[expect(dead_code)]
            trait AssertCTypeLastField {
                fn assert_ctype_last_field();
            }

            impl #impl_generics AssertCTypeLastField for #ident #ty_generics #where_clause {
                fn assert_ctype_last_field() {
                    const {
                        assert!(co3::impls!(#last: Copy | !Sized));
                    }
                }
            }
        };
    }
}

fn gen_last_field_copy_bound(generics: &syn::Generics, fields: &[&syn::Type]) -> TokenStream {
    if let Some(last) = fields.last()
        && !is_type_parameterized(last, generics)
    {
        return quote!(for<'dummy> #last: Copy,);
    }

    quote!()
}

fn gen_default_impl(ident: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics Default for #ident #ty_generics #where_clause {
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
    format_ident!("C{enum_name}V{variant_name}")
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
    field_types: &[&syn::Type],
    generics: &syn::Generics,
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
) -> TokenStream {
    let mut predicates = fields
        .iter()
        .filter(|&ty| is_type_parameterized(ty, generics))
        .map(|ty| quote!(#ty: co3::ExternC<CType: Copy>))
        .collect::<Vec<_>>();

    if ADD_COPY
        && let Some(last) = fields.last()
        && !is_type_parameterized(last, generics)
    {
        predicates.push(quote!(for<'_dummy> #last: co3::ExternC<CType: Copy>));
    };

    quote! { #( #predicates,)* }
}
