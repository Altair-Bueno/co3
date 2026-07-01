//! Structures and macros related to FFI and generation of FFI bindings. Any type that implements
//! [`ExternC`] can be used in the FFI bindings generated with [`export`]/[`extern_C!`]. It
//! is advisable to implement [`Ir`] and benefit from automatic implementation of [`ExternC`]
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc as alloc_crate;
extern crate self as co3;

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
use crate::{
    borrow::{BorrowCast, BorrowCastMut},
    boxed::{CBox, CBoxedSlice},
};
use crate::{
    ir::{NonRobust, ReprC, ReprFamily, ReprRust},
    niche::{NicheFamily, WithNiche, WithoutNiche},
    option::ReprCOption,
    result::ReprCResult,
    size::{MetaSized, SizeFamily, SliceLike, Thin, Wide},
    slice::{CSlice, CSliceMut},
    stored::{DecodeOwned, EmptyStore, EncodeOwned, ReprRustOrTransmutedNonRobust, Store},
};

#[cfg(feature = "alloc")]
pub mod alloc;
pub mod borrow;
#[cfg(feature = "alloc")]
pub mod boxed;
#[doc(hidden)]
pub mod either;
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
/// opaque/extern type pointers which don't also implement `RobustReprC` is very likely to cause UB.
///
/// # Safety
///
/// Type implementing the trait must have a guaranteed C ABI.
pub unsafe trait RobustReprC {}

/// `RobustReprC` type that is allowed as a C static.
///
/// # Safety
///
/// Type must be allowed as a C static.
pub unsafe trait CStatic: RobustReprC + Copy {}

/// `RobustReprC` type that is allowed as a C function argument.
///
/// # Safety
///
/// Type must be allowed as a C function argument type.
pub unsafe trait CFnArg: RobustReprC + Copy {}

/// `RobustReprC` type that is allowed as a C function return value.
///
/// # Safety
///
/// Type must be allowed as a C function return type.
pub unsafe trait CFnReturn: RobustReprC + Copy {}

unsafe impl<T: CFnArg> CStatic for T {}
unsafe impl<T: CFnArg, const N: usize> CStatic for [T; N] {}

unsafe impl<T: CFnArg> CFnReturn for T {}
unsafe impl CFnReturn for () {}

disjoint_impls! {
    /// A Rust type that has an `extern "C"` ABI
    pub trait ExternC {
        /// The C-compatible representation of this Rust type.
        type CType: RobustReprC + ?Sized;
    }

    impl<R: ReprFamily + SizeFamily<Kind: Thin> + ExternC + ?Sized> ExternC for &R
    where
        Self: ReprFamily<Kind: ReprRustOrTransmutedNonRobust>,
    {
        type CType = *const R::CType;
    }
    impl<R: ReprFamily<Kind = ReprC<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> ExternC for &R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Data: ExternC, Metadata = usize>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type CType = CSlice<<R::Data as ExternC>::CType>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ExternC for &R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: ToOwned<Owned: ExternC<CType: BorrowCast>>,
    {
        type CType = <<R::Owned as ExternC>::CType as BorrowCast>::AsConst;
    }

    impl<R: ReprFamily + SizeFamily<Kind: Thin> + ExternC + ?Sized> ExternC for &mut R
    where
        Self: ReprFamily<Kind: ReprRustOrTransmutedNonRobust>,
    {
        type CType = *mut R::CType;
    }
    impl<R: ReprFamily<Kind = ReprC<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> ExternC for &mut R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Data: ExternC, Metadata = usize>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type CType = CSliceMut<<R::Data as ExternC>::CType>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = MetaSized<K>> + ?Sized, K> ExternC for &mut R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: ToOwned<Owned: ExternC<CType: BorrowCastMut>>,
    {
        type CType = <<R::Owned as ExternC>::CType as BorrowCastMut>::AsMut;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily + SizeFamily<Kind = crate::size::Sized> + ExternC> ExternC for Box<R>
    where
        Self: ReprFamily<Kind: ReprRustOrTransmutedNonRobust>,
        <R as ExternC>::CType: Sized,
    {
        type CType = CBox<R::CType>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprC<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> ExternC for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Data: ExternC, Metadata = usize>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type CType = CBoxedSlice<<R::Data as ExternC>::CType>;
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
        <R as ExternC>::CType: Sized,
    {
        type CType = ReprCOption<R::CType>;
    }
    impl<R: NicheFamily<Kind: WithNiche> + ExternC> ExternC for Option<R> {
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
        type CType = ReprCResult<R::CType, E::CType>;
    }
    // TODO: Implement for niche optimized Results
}

disjoint_impls! {
    /// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
    pub trait Encode: EncodeOwned {
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.soft_encode_owned(store)
        }

        fn encode(self) -> Self::CType
        where
            Self::Store: EmptyStore,
        {
            self.encode_owned()
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized, K> Encode for Box<R>
    where
        Self: ReprFamily<Kind = ReprC<K>> + EncodeOwned,
    {}
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprC<K>> + ?Sized, K> Encode for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust> + EncodeOwned,
    {}
}

disjoint_impls! {
    /// Facilitates conversion into a Rust type from a corresponding C-compatible representation.
    pub trait Decode<'d>: DecodeOwned<'d> {
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { DecodeOwned::soft_decode_owned(source, store) }
        }

        /// Perform the conversion from [`Self::CType`] into [`Self`] without external storage.
        ///
        /// # Safety
        ///
        /// - All conversions from a pointer must ensure pointer validity beforehand
        unsafe fn decode(source: Self::CType) -> Option<Self>
        where
            Self::Store: EmptyStore + 'd,
        {
            unsafe { DecodeOwned::decode_owned(source) }
        }
    }

    #[cfg(feature = "alloc")]
    impl<'d, R: ?Sized, K> Decode<'d> for Box<R>
    where
        Self: ReprFamily<Kind = ReprC<K>> + DecodeOwned<'d>,
    {}
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = ReprC<K>> + ?Sized, K> Decode<'d> for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust> + DecodeOwned<'d>,
    {}
}

impl<R: ?Sized> Encode for &R where Self: EncodeOwned {}
impl<R: ?Sized> Encode for &mut R where Self: EncodeOwned {}

impl<'d, R: ?Sized> Decode<'d> for &'d R where Self: DecodeOwned<'d> {}
impl<'d, R: ?Sized> Decode<'d> for &'d mut R where Self: DecodeOwned<'d> {}

impl<R: Encode, const N: usize> Encode for [R; N] where Self: EncodeOwned {}
impl<'d, R: Decode<'d>, const N: usize> Decode<'d> for [R; N] where Self: DecodeOwned<'d> {}

impl<R: Encode> Encode for Option<R> where Self: EncodeOwned {}
impl<'d, R: Decode<'d>> Decode<'d> for Option<R> where Self: DecodeOwned<'d> {}

impl<R: Encode, E: Encode> Encode for Result<R, E> where Self: EncodeOwned {}
impl<'d, R: Decode<'d>, E: Decode<'d>> Decode<'d> for Result<R, E> where Self: DecodeOwned<'d> {}

#[cfg(feature = "alloc")]
impl<R: ExternC<CType: Sized>> ExternC for Vec<R> {
    type CType = CBoxedSlice<R::CType>;
}

#[cfg(feature = "alloc")]
impl<R> Encode for Vec<R>
where
    Self: EncodeOwned,
    Box<[R]>: Encode,
{
}
#[cfg(feature = "alloc")]
impl<'d, R> Decode<'d> for Vec<R>
where
    Self: DecodeOwned<'d>,
    Box<[R]>: Decode<'d>,
{
}

// TODO: Check https://github.com/mversic/co3/issues/13
const fn assert_arr_has_non_zero_len<const N: usize>() {
    assert!(N != 0, "empty array is a ZST");
}

#[cfg(test)]
#[cfg(feature = "alloc")]
mod tests {
    use super::*;

    #[test]
    fn encode_stored_mut_ref() {
        let inner = 8u8;
        let other = 42u8;
        let mut value = Some(inner);
        let value_mut_ref: &mut Option<u8> = &mut value;
        {
            let mut store = Box::default();
            let encoded = value_mut_ref.soft_encode(&mut *store);
            unsafe {
                *encoded = ReprCOption::Some(other);
            }
            store.sync().unwrap();
        }
        assert_eq!(value, Some(42u8));

        let mut slice = [Some(1u8)];
        let ref_mut: &mut [_] = &mut slice;
        {
            let mut store = Box::default();
            let encoded = ref_mut.soft_encode(&mut *store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };
            c_slice[0] = ReprCOption::Some(other);
            store.sync().unwrap();
        }
        assert_eq!(slice, [Some(42u8)]);
    }

    #[test]
    fn decode_stored_mut_ref() {
        let mut c_opt = ReprCOption::Some(1u8);
        let c_ptr: *mut _ = &mut c_opt;
        let new_val: u8 = 42;
        {
            let mut store = Box::default();
            let decoded = unsafe { <&mut _>::soft_decode(c_ptr, &mut *store) }.unwrap();

            *decoded = Some(new_val);
            store.sync().unwrap();
        }
        assert_eq!(c_opt, ReprCOption::Some(42u8));

        let mut c_opts = [ReprCOption::Some(1u8)];
        let c_slice = CSliceMut::from_slice(&mut c_opts);
        let x: u8 = 10;
        {
            let mut store = Box::default();
            let decoded = unsafe { <&mut [_]>::soft_decode(c_slice, &mut *store) }.unwrap();

            decoded[0] = Some(x);
            store.sync().unwrap();
        }
        assert_eq!(c_opts[0], ReprCOption::Some(10u8));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn encode_stored_ref_mut_slice() {
        use crate::tuple::ReprCTuple1;

        let mut tuples = [(1_u32,)];
        let slice_ref: &mut [_] = &mut tuples;

        {
            let mut store = Box::default();

            let encoded = slice_ref.soft_encode(&mut store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };

            c_slice[0] = ReprCTuple1(100);
            store.sync().unwrap();
        }

        assert_eq!(tuples[0].0, 100);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn decode_stored_ref_mut_slice() {
        use crate::tuple::ReprCTuple1;

        let mut tuples = [ReprCTuple1(10)];
        let c_slice = CSliceMut::from_slice(&mut tuples);

        {
            let mut store = Box::default();
            let decoded = unsafe { <&mut [(_,)]>::soft_decode(c_slice, &mut store) }.unwrap();

            decoded[0].0 = 100;
            store.sync().unwrap();
        }

        assert_eq!(tuples[0].0, 100);
    }
}
