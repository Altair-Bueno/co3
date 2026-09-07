use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};

pub(crate) fn gen_assertion(
    source_ty: &syn::Type,
    target_ty: &syn::Type,
    span: Span,
) -> TokenStream {
    quote_spanned! {span=>
        const {
            assert!(
                core::mem::size_of::<#source_ty>() == core::mem::size_of::<#target_ty>(),
                "ABI retype size mismatch",
            );
            assert!(
                core::mem::align_of::<#source_ty>() == core::mem::align_of::<#target_ty>(),
                "ABI retype alignment mismatch",
            );
        }
    }
}

pub(crate) fn gen_retype(
    source: TokenStream,
    source_ty: &syn::Type,
    target_ty: &syn::Type,
) -> TokenStream {
    let assertion = gen_assertion(source_ty, target_ty, Span::call_site());
    quote! {{
        #assertion
        let __co3_abi_retype_source = core::mem::ManuallyDrop::new(#source);
        unsafe {
            core::mem::transmute_copy::<#source_ty, #target_ty>(
                &*__co3_abi_retype_source,
            )
        }
    }}
}
