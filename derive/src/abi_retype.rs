use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};

pub(crate) fn gen_assertion(
    source_ty: &syn::Type,
    target_ty: &syn::Type,
    span: Span,
) -> TokenStream {
    quote_spanned! {span=>
        const {
            if core::mem::size_of::<#source_ty>() != core::mem::size_of::<#target_ty>() {
                panic!("ABI retype size mismatch");
            }
            if core::mem::align_of::<#source_ty>() != core::mem::align_of::<#target_ty>() {
                panic!("ABI retype alignment mismatch");
            }
        }
    }
}

pub(crate) fn gen_forced_assertion(
    source_ty: &syn::Type,
    target_ty: &syn::Type,
    span: Span,
) -> TokenStream {
    quote_spanned! {span=>
        let _: [(); const {
            if core::mem::size_of::<#source_ty>() != core::mem::size_of::<#target_ty>() {
                panic!("ABI retype size mismatch");
            }
            if core::mem::align_of::<#source_ty>() != core::mem::align_of::<#target_ty>() {
                panic!("ABI retype alignment mismatch");
            }
            0
        }] = [];
    }
}

pub(crate) fn gen_retype(
    source: TokenStream,
    source_ty: &syn::Type,
    target_ty: &syn::Type,
) -> TokenStream {
    let assertion = gen_assertion(source_ty, target_ty, Span::call_site());
    let retype = gen_retype_after_check(source, source_ty, target_ty);
    quote! {{
        #assertion
        #retype
    }}
}

/// Generates an ABI retype whose layout has already been established by a
/// declaration-level assertion.
pub(crate) fn gen_retype_after_check(
    source: TokenStream,
    source_ty: &syn::Type,
    target_ty: &syn::Type,
) -> TokenStream {
    quote! {{
        let __co3_abi_retype_source = core::mem::ManuallyDrop::new(#source);
        unsafe {
            core::mem::transmute_copy::<#source_ty, #target_ty>(
                &*__co3_abi_retype_source,
            )
        }
    }}
}
