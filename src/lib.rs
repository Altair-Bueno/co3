//! Structures and macros related to FFI and generation of FFI bindings. Any type that implements
//! [`ExternC`] can be used in the FFI bindings generated with [`export`]/[`extern_C!`]. It
//! is advisable to implement [`Ir`] and benefit from automatic implementation of [`ExternC`]
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc as alloc_crate;
extern crate self as co3;

use core::cell::UnsafeCell;

#[cfg(feature = "alloc")]
use alloc_crate::{borrow::ToOwned, boxed::Box, vec::Vec};

#[cfg(feature = "derive")]
pub use co3_derive::*;
use derive_more::Display;
use disjoint_impls::disjoint_impls;
// TODO: I don't like having to reexport macros from other crates
#[doc(hidden)]
pub use impls::impls;

#[cfg(feature = "alloc")]
use crate::boxed::{CBox, CBoxedSlice};
use crate::{
    borrow::BorrowCast,
    ir::{NonRobust, ReprFamily, ReprRust, Transmuted},
    niche::{Niche, NicheFamily, WithCustomNiche, WithoutNiche},
    option::COption,
    out_ptr::Zst,
    result::CResult,
    size::{MetaSized, SizeFamily, SliceLike, Thin, Wide},
    slice::{CSlice, CSliceMut},
    stored::{SoftDecodeOwned, SoftEncodeOwned, Store},
};

#[cfg(feature = "alloc")]
pub mod alloc;
pub mod borrow;
#[cfg(feature = "alloc")]
pub mod boxed;
pub mod handle;
pub mod ir;
pub mod niche;
pub mod option;
pub mod out_ptr;
mod primitives;
pub mod result;
pub mod size;
pub mod slice;
mod std_impls;
pub mod stored;
pub mod transmute;
pub mod tuple;

/// Result of execution of an FFI function
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum FfiReturn {
    /// FFI function failed during the execution of the wrapped method on the provided handle.
    ExecutionFail = -4,
    /// FFI function execution panicked.
    UnrecoverableError = -3,
    /// The input argument provided to FFI function contains a trap representation.
    TrapRepresentation = -2,
    /// Provided handle id doesn't match any known handles.
    UnknownHandle = -1,
    /// FFI function executed successfully.
    Ok = 0,
}

/// Robust type that conforms to C ABI and can be safely shared across FFI boundaries.
///
/// Note that, for raw pointers, ABI compatibility of referent is not guaranteed. Dereferencing
/// opaque/extern type pointers which don't also implement `ReprC` is very likely to cause UB.
///
/// # Safety
///
/// Type implementing the trait must have a guaranteed C ABI.
pub unsafe trait ReprC {}

/// `ReprC` type that is allowed as a C static.
///
/// # Safety
///
/// Type must be allowed as a C static.
pub unsafe trait CStatic: ReprC + Copy {}

/// `ReprC` type that is allowed as a C function argument.
///
/// # Safety
///
/// Type must be allowed as a C function argument type.
pub unsafe trait CFnArg: ReprC + Copy {}

/// `ReprC` type that is allowed as a C function return value.
///
/// # Safety
///
/// Type must be allowed as a C function return type.
pub unsafe trait CFnReturn: ReprC + Copy {}

unsafe impl<T: CFnArg> CStatic for T {}
unsafe impl<T: CFnArg, const N: usize> CStatic for [T; N] {}

unsafe impl<T: CFnArg> CFnReturn for T {}
unsafe impl CFnReturn for () {}

/// Refer to [`SoftEncode`]
pub trait Encode: SoftEncode {
    fn encode<'itm>(self) -> Self::CType
    where
        Self: 'itm;
}

/// Refer to [`SoftDecode`]
pub trait Decode<'d>: SoftDecode<'d> {
    /// Perform the conversion from [`Self::CType`] into [`Self`]
    ///
    /// # Safety
    ///
    /// - All conversions from a pointer must ensure pointer validity beforehand
    unsafe fn decode(source: Self::CType) -> Option<Self>;
}

pub trait Mutability {
    type Kind;
}

pub enum InteriorMutability {}
pub enum ExteriorMutability {}

impl<R> Mutability for UnsafeCell<R> {
    type Kind = InteriorMutability;
}

disjoint_impls! {
    /// A Rust type that has an `extern "C"` ABI
    pub trait ExternC {
        /// The C-compatible representation of this Rust type.
        type CType: ReprC + ?Sized;
    }

    impl<R: ExternC + ?Sized> ExternC for &R
    where
        Self: ReprFamily<Kind = Transmuted<NonRobust>>,
    {
        type CType = *const R::CType;
    }
    //impl<R: Mutability<Kind = InteriorMutability> + ExternC + ?Sized> ExternC for &R
    //where
    //    Self: ReprFamily<Kind = Transmuted<NonRobust>>,
    //{
    //    type CType = *mut R::CType;
    //}
    impl<R: ReprFamily<Kind = Transmuted<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> ExternC for &R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Data: ExternC, Metadata = usize>,
        <<R as Wide>::Data as ExternC>::CType: Copy,
    {
        type CType = CSlice<<R::Data as ExternC>::CType>;
    }
    //impl<R: ReprFamily<Kind = Transmuted<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> ExternC for &R
    //where
    //    Self: ReprFamily<Kind = ReprRust>,
    //    R: Mutability<Kind = InteriorMutability>,
    //    R: Wide<Data: ExternC, Metadata = usize>,
    //    <<R as Wide>::Data as ExternC>::CType: Copy,
    //{
    //    type CType = CSliceMut<<R::Data as ExternC>::CType>;
    //}
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind: Thin> + ExternC + ?Sized> ExternC for &R
    where
        Self: ReprFamily<Kind = ReprRust>,
    {
        type CType = *const R::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ExternC for &R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: ToOwned<Owned: ExternC<CType: BorrowCast>>,
    {
        type CType = <<R::Owned as ExternC>::CType as BorrowCast>::AsConst;
    }

    impl<R: ExternC + ?Sized> ExternC for &mut R
    where
        Self: ReprFamily<Kind = Transmuted<NonRobust>>,
    {
        type CType = *mut R::CType;
    }
    impl<R: ReprFamily<Kind = Transmuted<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> ExternC for &mut R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Data: ExternC, Metadata = usize>,
        <<R as Wide>::Data as ExternC>::CType: Copy,
    {
        type CType = CSliceMut<<R::Data as ExternC>::CType>;
    }
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind: Thin> + ExternC + ?Sized> ExternC for &mut R
    where
        Self: ReprFamily<Kind = ReprRust>,
    {
        type CType = *mut R::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ExternC for &mut R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: ToOwned<Owned: ExternC<CType: BorrowCast>>,
    {
        type CType = <<R::Owned as ExternC>::CType as BorrowCast>::AsMut;
    }

    #[cfg(feature = "alloc")]
    impl<R: ExternC> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = Transmuted<NonRobust>>,
        <R as ExternC>::CType: Copy,
    {
        type CType = CBox<R::CType>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Data: ExternC, Metadata = usize>,
        <<R as Wide>::Data as ExternC>::CType: Copy,
    {
        type CType = CBoxedSlice<<R::Data as ExternC>::CType>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = crate::size::Sized> + ExternC> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust>,
        <R as ExternC>::CType: Copy,
    {
        type CType = CBox<R::CType>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: ToOwned<Owned: ExternC>,
    {
        type CType = <R::Owned as ExternC>::CType;
    }

    impl<R: NicheFamily<Kind = WithoutNiche> + ExternC> ExternC for Option<R>
    where
        Self: ReprFamily<Kind = ReprRust>,
        <R as ExternC>::CType: Copy,
    {
        type CType = COption<R::CType>;
    }
    impl<R: NicheFamily<Kind = WithCustomNiche> + Niche> ExternC for Option<R>
    where
        Self: ReprFamily<Kind = ReprRust>,
    {
        type CType = R::CType;
    }

    impl<
        R: NicheFamily<Kind = WithoutNiche> + ExternC,
        E: NicheFamily<Kind = WithoutNiche> + ExternC,
    >
        ExternC for Result<R, E>
    where
        Self: ReprFamily<Kind = ReprRust>,
        <R as ExternC>::CType: Copy,
        <E as ExternC>::CType: Copy,
    {
        type CType = CResult<R::CType, E::CType>;
    }
    // TODO: Implement for niche optimized Results
}

disjoint_impls! {
    /// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
    pub trait SoftEncode: SoftEncodeOwned {
        /// Convert from [`Self`] into [`Self::CType`].
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm
        {
            SoftEncodeOwned::soft_encode(self, store)
        }
    }

    // FIXME: we'll have to inline this impls almost certainly. Look at raw pointers
    impl<R, K> SoftEncode for R
    where
        R: ReprFamily<Kind = Transmuted<K>> + SoftEncodeOwned,
    {}

    impl<'a, R: ?Sized> SoftEncode for &'a R
    where
        Self: ReprFamily<Kind = ReprRust> + SoftEncodeOwned,
    {}

    impl<'a, R: ?Sized> SoftEncode for &'a mut R
    where
        Self: ReprFamily<Kind = ReprRust> + SoftEncodeOwned,
    {}

    impl<R: ReprC + ?Sized> SoftEncode for *const R
    where
        Self: ReprFamily<Kind = ReprRust> + SoftEncodeOwned,
    {}

    impl<R: ReprC + ?Sized> SoftEncode for *mut R
    where
        Self: ReprFamily<Kind = ReprRust> + SoftEncodeOwned,
    {}

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> SoftEncode for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust> + SoftEncodeOwned,
    {}
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + SoftEncode> SoftEncode for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust> + SoftEncodeOwned,
    {}

    impl<R: SoftEncode, const N: usize> SoftEncode for [R; N]
    where
        Self: ReprFamily<Kind = ReprRust> + SoftEncodeOwned,
    {}

    impl<R: SoftEncode> SoftEncode for Option<R>
    where
        Self: ReprFamily<Kind = ReprRust> + SoftEncodeOwned,
    {}

    impl<R: SoftEncode, E: SoftEncode> SoftEncode for Result<R, E>
    where
        Self: ReprFamily<Kind = ReprRust> + SoftEncodeOwned,
    {}
    // TODO: Implement for niche optimized Results
}

disjoint_impls! {
    /// Facilitates conversion into a Rust type from a corresponding C-compatible representation.
    pub trait SoftDecode<'d>: SoftDecodeOwned<'d> {

        /// Perform the conversion from [`Self::CType`] into [`Self`]
        ///
        /// # Safety
        ///
        /// - All conversions from a pointer must ensure pointer validity beforehand
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { SoftDecodeOwned::soft_decode(source, store) }
        }
    }

    impl<'d, R, K> SoftDecode<'d> for R
    where
        R: ReprFamily<Kind = Transmuted<K>> + SoftDecodeOwned<'d>,
    {}

    impl<'d, R: ?Sized> SoftDecode<'d> for &'d R
    where
        Self: ReprFamily<Kind = ReprRust> + SoftDecodeOwned<'d>,
    {}

    impl<'d, R: ?Sized> SoftDecode<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = ReprRust> + SoftDecodeOwned<'d>,
    {}

    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = Transmuted<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> SoftDecode<'d> for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust> + SoftDecodeOwned<'d>,
    {}
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = ReprRust> + SoftDecode<'d>> SoftDecode<'d> for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust> + SoftDecodeOwned<'d>,
    {}

    impl<'d, R: SoftDecode<'d>, const N: usize> SoftDecode<'d> for [R; N]
    where
        Self: ReprFamily<Kind = ReprRust> + SoftDecodeOwned<'d>,
    {}

    impl<'d, R: SoftDecode<'d>> SoftDecode<'d> for Option<R>
    where
        Self: ReprFamily<Kind = ReprRust> + SoftDecodeOwned<'d>,
    {}

    impl<'d, R: SoftDecode<'d>, E: SoftDecode<'d>> SoftDecode<'d> for Result<R, E>
    where
        Self: ReprFamily<Kind = ReprRust> + SoftDecodeOwned<'d>,
    {}
    // TODO: Implement for niche optimized Results
}

#[cfg(feature = "alloc")]
impl<R: ExternC<CType: Copy>> ExternC for Vec<R> {
    type CType = CBoxedSlice<R::CType>;
}

#[cfg(feature = "alloc")]
impl<R> SoftEncode for Vec<R>
where
    Self: SoftEncodeOwned<Store = <Box<[R]> as SoftEncodeOwned>::Store>,
    Self: ExternC<CType = <Box<[R]> as ExternC>::CType>,
    Box<[R]>: SoftEncode,
{
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        SoftEncode::soft_encode(self.into_boxed_slice(), store)
    }
}

#[cfg(feature = "alloc")]
impl<'d, R> SoftDecode<'d> for Vec<R>
where
    Self: SoftDecodeOwned<'d, Store = <Box<[R]> as SoftDecodeOwned<'d>>::Store>,
    Self: ExternC<CType = <Box<[R]> as ExternC>::CType>,
    Box<[R]>: SoftDecode<'d>,
{
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { <Box<[R]> as SoftDecode<'d>>::soft_decode(source, store) }.map(Into::into)
    }
}

impl<R: SoftEncode<Store: Zst>> Encode for R {
    fn encode<'itm>(self) -> Self::CType
    where
        Self: 'itm,
    {
        let mut store = Default::default();
        SoftEncodeOwned::soft_encode(self, &mut store)
    }
}

impl<'d, R: SoftDecode<'d, Store: Zst> + 'd> Decode<'d> for R {
    unsafe fn decode(source: Self::CType) -> Option<Self> {
        let mut store = Default::default();
        // SAFETY: `Decode` is only blanket-implemented for zero-sized stores, so extending the
        // borrow of the local store does not extend the lifetime of any backing data.
        let store = unsafe { core::mem::transmute::<&mut R::Store, &'d mut R::Store>(&mut store) };
        unsafe { <Self as SoftDecode>::soft_decode(source, store) }
    }
}

/// Macro for defining FFI types of a known category ([`crate::ir::Robust`], [`Transmuted`] or [`ReprRust`]).
///
/// The implementation for an FFI type of one of the categories incurs a lot of bloat that
/// is reduced by the use of this macro
///
/// # Safety
///
/// * [`crate::ir::Robust`] derives [`ReprC`]. Check safety invariants for [`ReprC`]
/// * [`Transmuted`] derives [`CheckedTransmute`]. Check safety invariants for [`CheckedTransmute`]
///
/// # Example
///
/// ```
/// use co3::{
///     borrow::{EncodeAsRef, SoftDecodeView},
///     ir::{SizeFamily, Sized},
///     reprC
/// };
///
/// #[repr(C)]
/// #[derive(Clone, Copy)]
/// struct Robust(u64, i32);
///
/// #[repr(transparent)]
/// struct MyPtr<T>(*mut T);
///
/// #[repr(transparent)]
/// struct Wrapper(u32);
///
/// struct ReprRust<T: ?Sized>(u64, T);
///
/// co3::reprC! {
///     // SAFETY: Type MUST NOT have traps
///     unsafe impl Robust for Robust {}
/// }
///
/// co3::reprC! {
///     // SAFETY: `Self::is_valid` must not return false posives
///     unsafe impl(T) Transparent for MyPtr<T> where (T: Copy) {
///         const NICHE_VALUE: Self::CType = core::ptr::null_mut();
///
///         type Target = $target:ty;
///         fn is_valid(target: &Self::CType) -> bool {
///             !target.is_null()
///         }
///     }
/// }
///
/// // If validation fn or niche value is given,
/// // wrapper type delegates to the inner type
/// co3::reprC! {
///     unsafe impl Transparent for Wrapper {
///         type Target = u32;
///     }
/// }
///
/// co3::reprC! {
///     // To use this type one still has to implement
///     // a suite of additional conversion traits
///     impl(T: ?Sized) ReprRust for ReprRust<T> {}
/// }
///
///
/// // Some extra glue that is required:
///
/// impl<T> Drop for MyPtr<T> {
///     fn drop(&mut self) {
///         unimplemented!("Do a cleanup")
///     }
/// }
///
/// impl SizeFamily for Robust {
///     type Kind = Sized;
/// }
/// impl<T: ?Sized + SizeFamily> SizeFamily for ReprRust<T> {
///     type Kind = T::Kind;
/// }
///
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! reprC {
    (impl $(( $($params:tt)* ))? ReprRust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        impl<$($($params)*)?> $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::ir::ReprRust;
        }
    };

    (unsafe impl $(())? Robust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @robust_common [for<'_dummy> Self: Copy,] [] $self_ty $([$($preds)*])? }
    };
    (unsafe impl ( $($params:tt)+ ) Robust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @robust_common [Self: Copy,] [$($params)+] $self_ty $([$($preds)*])? }
    };

    (unsafe impl $(( $($params:tt)+ ))? SizedRobust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @sized_size_family [$($($params)+)?] $self_ty $([$($preds)*])? }
        // TODO: Self: Copy is here just to satisfy for CFnArg. find different approach
        $crate::reprC! { @robust_common [Self: Copy,] [$($($params)+)?] $self_ty $([$($preds)*])? }
    };
    (unsafe impl $(())? SizedRobust for $self_ty:ty $(where ($($preds:tt)*))? {}) => {
        $crate::reprC! { @sized_size_family [] $self_ty $([$($preds)*])? }
        $crate::reprC! { @robust_common [] [] $self_ty $([$($preds)*])? }
    };

    (@robust_common [$($copy_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        // TODO: How can Robust types implement Drop if they are Copy? ?Sized can implement Drop
        $crate::reprC! { @assert_no_drop [$($impl_generics)*] $self_ty $([$($preds)*])? }

        const _: () = {
            #[expect(dead_code)]
            trait AssertNonZst {
                fn assert_non_zst();
            }

            impl<$($impl_generics)*> AssertNonZst for $self_ty where $($copy_bound)* $($($preds)*)? {
                fn assert_non_zst() {
                    const {
                        assert!(
                            core::mem::size_of::<Self>() != 0,
                            "custom ZSTs are not supported yet"
                        );
                    }
                }
            }
        };

        unsafe impl<$($impl_generics)*> $crate::ReprC for $self_ty where $($($preds)*)? {}

        unsafe impl<$($impl_generics)*> $crate::CFnArg for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
        {}

        impl<$($impl_generics)*> $crate::ExternC for $self_ty where $($($preds)*)? {
            type CType = Self;
        }

        unsafe impl<$($impl_generics)*> $crate::transmute::CheckedTransmute for $self_ty $(where $($preds)*)? {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        impl<$($impl_generics)*> $crate::stored::SoftEncodeOwned for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
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

        impl<'d, $($impl_generics)*> $crate::stored::SoftDecodeOwned<'d> for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
        {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl<$($impl_generics)*> $crate::ir::ReprFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::ir::Transmuted<$crate::ir::Robust>;
        }

        impl<$($impl_generics)*> $crate::niche::NicheFamily for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
        {
            type Kind = $crate::niche::WithoutNiche;
        }

        impl<$($impl_generics)*> $crate::borrow::Borrow for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
        {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Owner = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> <Self as $crate::borrow::Borrow>::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }

        impl<'itm, $($impl_generics)*> $crate::borrow::ToOwned<'itm> for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
        {
            #[inline(always)]
            fn to_owned(source: Self) -> Self {
                source
            }
        }

        unsafe impl<$($impl_generics)*> $crate::handle::Erase for $self_ty $(where $($preds)*)? {
            type Erased = Self;
        }

        unsafe impl<$($impl_generics)*> $crate::borrow::BorrowCast for $self_ty where
            $($copy_bound)*
            $($($preds)*)?
        {
            type AsConst = Self;
            type AsMut = Self;
        }
    };

    (unsafe impl $(())? Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        impl $crate::size::SizeFamily for $self_ty where $($($preds)*)? {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_valid [for<'_dummy>] [for<'_dummy> Self: Sized,] [] $self_ty [$target] $([$($preds)*])? {}
        }

    };
    (unsafe impl ( $($params:tt)+ ) Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        impl<$($params)*> $crate::size::SizeFamily for $self_ty where
            $target: $crate::size::SizeFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_valid [] [Self: Sized,] [$($params)+] $self_ty [$target] $([$($preds)*])? {}
        }

    };

    (unsafe impl $(())? NoDropSizedTransparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_delegate_niche_valid [for<'_dummy>] [] [] $self_ty [$target] $([$($preds)*])? {}
        }
    };
    (unsafe impl ( $($params:tt)+ ) NoDropSizedTransparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [$($params)+] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_delegate_niche_valid [] [] [$($params)+] $self_ty [$target] $([$($preds)*])? {}
        }
    };

    (unsafe impl $(())? Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl $crate::size::SizeFamily for $self_ty where $($($preds)*)? {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_sized [] [for<'_dummy> Self: Sized,] [] $self_ty [$target] $([$($preds)*])? {
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

    };
    (unsafe impl ( $($params:tt)+ ) Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl<$($params)+> $crate::size::SizeFamily for $self_ty where
            $target: $crate::size::SizeFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_delegate_niche_sized [] [Self: Sized,] [$($params)+] $self_ty [$target] $([$($preds)*])? {
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

    };

    (unsafe impl $(())? NoDropSizedTransparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_delegate_niche_sized [for<'_dummy>] [] [] $self_ty [$target] $([$($preds)*])? {
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };
    (unsafe impl ( $($params:tt)+ ) NoDropSizedTransparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        type Target = $target:ty;

        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [$($params)+] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_delegate_niche_sized [] [] [$($params)+] $self_ty [$target] $([$($preds)*])? {
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };

    (unsafe impl $(())? Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;

        type Target = $target:ty;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl $crate::size::SizeFamily for $self_ty where $($($preds)*)? {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_explicit_niche [for<'_dummy>] [for<'_dummy> Self: Sized,] [] $self_ty [$target] $([$($preds)*])? {
                const NICHE_VALUE: $niche_ty = $niche_value;

                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

    };
    (unsafe impl ( $($params:tt)+ ) Transparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;

        type Target = $target:ty;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        impl<$($params)+> $crate::size::SizeFamily for $self_ty where
            $target: $crate::size::SizeFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::size::SizeFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_explicit_niche [] [Self: Sized,] [$($params)+] $self_ty [$target] $([$($preds)*])? {
                const NICHE_VALUE: $niche_ty = $niche_value;

                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

    };

    (unsafe impl $(())? NoDropSizedTransparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;

        type Target = $target:ty;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_explicit_niche [for<'_dummy>] [] [] $self_ty [$target] $([$($preds)*])? {
                const NICHE_VALUE: $niche_ty = $niche_value;

                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };
    (unsafe impl ( $($params:tt)+ ) NoDropSizedTransparent for $self_ty:ty $(where ( $($preds:tt)* ))? {
        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;

        type Target = $target:ty;
        fn is_valid($target_var:ident: $target_ty:ty) -> $ret_val:ty
            $block:block
    }) => {
        $crate::reprC! { @no_drop_sized_transmuted [$($params)+] $self_ty $([$($preds)*])? {} }

        $crate::reprC! {
            @transmuted_explicit_niche [] [] [$($params)+] $self_ty [$target] $([$($preds)*])? {
                const NICHE_VALUE: $niche_ty = $niche_value;

                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }
    };

    (@no_drop_sized_transmuted [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])? {}) => {
        $crate::reprC! { @sized_size_family [$($impl_generics)*] $self_ty $([$($preds)*])? }
        $crate::reprC! { @assert_no_drop [$($impl_generics)*] $self_ty $([$($preds)*])? }
    };

    (@transmuted_delegate_niche_valid [$($for_dummy:tt)*] [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty [$target:ty] $([$($preds:tt)*])? {}) => {
        $crate::reprC! {
            @transmuted_delegate_niche_sized [$($for_dummy)*] [$($sized_bound)*] [$($impl_generics)*] $self_ty [$target] $([$($preds)*])? {
                fn is_valid(_target: &Self::CType) -> bool {
                    unsafe { <$target as $crate::transmute::CheckedTransmute>::is_valid(*$target_ty) }
                }
            }
        }
    };

    (@transmuted_delegate_niche_sized [$($for_dummy:tt)*] [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty [$target:ty] $([$($preds:tt)*])? {
        fn is_valid($target_var:ident: $target_ty:ty) -> bool $block:block
    }) => {
        unsafe impl<$($impl_generics)*> $crate::ReprC for $self_ty where
            $($for_dummy)* $target: $crate::ReprC,
            $($($preds)*)?
        {}

        unsafe impl<$($impl_generics)*> $crate::CFnArg for $self_ty where
            $($for_dummy)* $target: $crate::CFnArg,
            $($($preds)*)?
        {}

        impl<$($impl_generics)*> $crate::ir::ReprFamily for $self_ty where
            $target: $crate::ir::ReprFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::ir::ReprFamily>::Kind;
        }

        impl<$($impl_generics)*> $crate::ir::EncodeReprFamily for $self_ty where
            $target: $crate::ir::EncodeReprFamily,
            $($($preds)*)?
        {
            type Kind = <$target as $crate::ir::EncodeReprFamily>::Kind;
        }

        impl<$($impl_generics)*> $crate::niche::NicheFamily for $self_ty where
            $($for_dummy)* $target: $crate::niche::NicheFamily,
            $($sized_bound)*
            $($($preds)*)?
        {
            type Kind = <$target as $crate::niche::NicheFamily>::Kind;
        }

        $crate::reprC! {
            @transmuted_common [$($for_dummy)*] [$($impl_generics)*] $self_ty [$target] $([$($preds)*])? {
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

        impl<$($impl_generics)*> $crate::niche::Niche for $self_ty where
            Self: $crate::ExternC<CType = <$target as $crate::ExternC>::CType>,
            $($for_dummy)* $target: $crate::niche::Niche,
            $($sized_bound)*
            $($($preds)*)?
        {
            const NICHE_VALUE: <Self as $crate::ExternC>::CType = <$target as $crate::niche::Niche>::NICHE_VALUE;
        }

        unsafe impl<$($impl_generics)*> $crate::niche::StableNiche for $self_ty
        where
            $($for_dummy)* $target: $crate::niche::StableNiche,
            Self: $crate::niche::Niche,
            $($sized_bound)*
            $($($preds)*)?
        {}
    };

    (@transmuted_explicit_niche [$($for_dummy:tt)*] [$($sized_bound:tt)*] [$($impl_generics:tt)*] $self_ty:ty [$target:ty] $([$($preds:tt)*])? {
        const NICHE_VALUE: $niche_ty:ty = $niche_value:expr;

        fn is_valid($target_var:ident: $target_ty:ty) -> bool $block:block
    }) => {
        impl<$($impl_generics)*> $crate::ir::ReprFamily for $self_ty where
            $target: $crate::ir::ReprFamily,
            $($($preds)*)?
        {
            type Kind = <$crate::ir::Transmuted<$crate::ir::NonRobust> as core::ops::Add<<$target as $crate::ir::ReprFamily>::Kind>>::Output;
        }

        //impl<$($impl_generics)*> $crate::ir::EncodeReprFamily for $self_ty where
        //    $target: $crate::ir::EncodeReprFamily,
        //    $($($preds)*)?
        //{
        //    type Kind = <$target as $crate::ir::EncodeReprFamily>::Kind;
        //}

        impl<$($impl_generics)*> $crate::niche::NicheFamily for $self_ty where
            $($sized_bound)*
            $($($preds)*)?
        {
            type Kind = $crate::niche::WithCustomNiche;
        }

        $crate::reprC! {
            @transmuted_common [$($for_dummy)*] [$($impl_generics)*] $self_ty [$target] $([$($preds)*])? {
                fn is_valid($target_var: $target_ty) -> bool $block
            }
        }

        impl<$($impl_generics)*> $crate::niche::Niche for $self_ty where
            $($sized_bound)*
            $($($preds)*)?
        {
            const NICHE_VALUE: $niche_ty = {
                assert!($crate::impls!
                    ($target: $crate::niche::NicheFamily<Kind = $crate::niche::WithoutNiche>),
                    "Transmuted CAN'T define a custom niche if target has a niche"
                );

                $niche_value
            };
        }
    };

    (@transmuted_common [$($for_dummy:tt)*] [$($impl_generics:tt)*] $self_ty:ty [$target:ty] $([$($preds:tt)*])? {
        fn is_valid($target_var:ident: $target_ty:ty) -> bool $block:block
    }) => {
        unsafe impl<'d, $($impl_generics)*> $crate::transmute::CheckedTransmute for $self_ty where
            &'d $target_ty: $crate::Decode<'d, CType = *const <$target_ty as $crate::ExternC>::CType>,
            $($for_dummy)* $target: $crate::transmute::CheckedTransmute,
            $($($preds)*)?
        {
            #[inline(always)]
            unsafe fn is_valid($target_var: &$target_ty) -> bool {
                let Some($target_var) = (unsafe { <&$target_ty as $crate::Decode>::decode($target_var) }) else {
                    return false;
                };

                $block
            }
        }

        impl<$($impl_generics)*> $crate::ExternC for $self_ty where $($($preds)*)? {
            type CType = <$target as $crate::ExternC>::CType;
        }

        unsafe impl<$($impl_generics)*> $crate::handle::Erase for $self_ty where
            $($for_dummy)* $target: $crate::handle::Erase,
            $($($preds)*)?
        {
            type Erased = <$target as $crate::handle::Erase>::Erased;
        }
    };

    (@assert_no_drop [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        const _: () = {
            #[expect(dead_code)]
            trait AssertNoDrop {
                fn assert_no_drop();
            }

            impl<$($impl_generics)*> AssertNoDrop for $self_ty $(where $($preds)*)? {
                fn assert_no_drop() {
                    const { assert!($crate::impls!(Self: !Drop)); }
                }
            }
        };
    };

    (@assert_no_drop [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        const _: () = {
            #[expect(dead_code)]
            trait AssertNoDrop {
                fn assert_no_drop();
            }

            impl<$($impl_generics)*> AssertNoDrop for $self_ty $(where $($preds)*)? {
                fn assert_no_drop() {
                    const {
                        // TODO: This is heuristic so it might
                        // make sense to reintroduce `DropFamily`
                        assert!(!core::mem::needs_drop::<Self>());
                    }
                }
            }
        };
    };

    (@sized_size_family [$($impl_generics:tt)*] $self_ty:ty $([$($preds:tt)*])?) => {
        impl<$($impl_generics)*> $crate::size::SizeFamily for $self_ty $(where $($preds)*)? {
            type Kind = $crate::size::Sized;
        }
    };
}

// TODO: Check https://github.com/mversic/co3/issues/13
const fn assert_arr_has_non_zero_len<const N: usize>() {
    assert!(N != 0, "empty array is a ZST");
}

#[cfg(test)]
mod tests {
    use static_assertions::assert_impl_all;

    use super::*;
    use crate::{
        ir::Robust,
        niche::{Niche, NicheFamily, StableNiche, WithStableNiche},
    };

    #[test]
    fn robust_u8() {
        assert_impl_all!(u8:
            ReprFamily<Kind = Transmuted<Robust>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = u8>,
            SoftDecode<'static>,
            SoftEncode,
            ReprC,
        );
        assert_impl_all!(&u8:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut u8:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            SoftDecode<'static>,
            SoftEncode,
        );
        // FIXME:
        //assert_impl_all!(Box<u8>:
        //    ReprFamily<Kind = Transmuted<Robust>>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut u8>,
        //    SoftDecode<'static>,
        //    SoftEncode,
        //);
        assert_impl_all!(&[u8]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut [u8]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[u8]>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //SoftDecode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<u8>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!([u8; 2]:
            ReprFamily<Kind = Transmuted<Robust>>,
            NicheFamily<Kind = WithoutNiche>,
            SoftDecode<'static>,
            SoftEncode,
            ReprC,
        );
        assert_impl_all!(Option<u8>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = COption<u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
    }

    #[test]
    fn encode_stored_mut_ref() {
        let inner = 8u8;
        let other = 42u8;
        let mut value = Some(inner);
        let value_mut_ref: &mut Option<u8> = &mut value;
        {
            let mut store = Box::default();
            let encoded = SoftEncode::soft_encode(value_mut_ref, &mut *store);
            unsafe {
                *encoded = COption::Some(other);
            }
            store.sync().unwrap();
        }
        assert_eq!(value, Some(42u8));

        let mut slice = [Some(1u8)];
        let ref_mut: &mut [_] = &mut slice;
        {
            let mut store = Box::default();
            let encoded = SoftEncode::soft_encode(ref_mut, &mut *store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };
            c_slice[0] = COption::Some(other);
            store.sync().unwrap();
        }
        assert_eq!(slice, [Some(42u8)]);
    }

    #[test]
    fn decode_stored_mut_ref() {
        let mut c_opt = COption::Some(1u8);
        let c_ptr: *mut _ = &mut c_opt;
        let new_val: u8 = 42;
        {
            let mut store = Box::default();
            let decoded =
                unsafe { <&mut Option<u8> as SoftDecode>::soft_decode(c_ptr, &mut *store) }
                    .unwrap();
            *decoded = Some(new_val);
            store.sync().unwrap();
        }
        assert_eq!(c_opt, COption::Some(42u8));

        let mut c_opts = [COption::Some(1u8)];
        let c_slice = CSliceMut::from_slice(Some(&mut c_opts));
        let x: u8 = 10;
        {
            let mut store = Box::default();
            let decoded =
                unsafe { <&mut [Option<u8>] as SoftDecode>::soft_decode(c_slice, &mut *store) }
                    .unwrap();
            decoded[0] = Some(x);
            store.sync().unwrap();
        }
        assert_eq!(c_opts[0], COption::Some(10u8));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn encode_stored_ref_mut_slice() {
        use crate::tuple::CTuple1;

        let mut tuples = [(1_u32,)];
        let slice_ref: &mut [_] = &mut tuples;

        {
            let mut store = Box::default();

            let encoded = SoftEncode::soft_encode(slice_ref, &mut store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };

            c_slice[0] = CTuple1(100);
            store.sync().unwrap();
        }

        assert_eq!(tuples[0].0, 100);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn decode_stored_ref_mut_slice() {
        use crate::tuple::CTuple1;

        let mut tuples = [CTuple1(10)];
        let c_slice = CSliceMut::from_slice(Some(&mut tuples));

        {
            let mut store = Box::default();
            let decoded =
                unsafe { <&mut [(_,)] as SoftDecode>::soft_decode(c_slice, &mut store) }.unwrap();

            decoded[0].0 = 100;
            store.sync().unwrap();
        }

        assert_eq!(tuples[0].0, 100);
    }
}
