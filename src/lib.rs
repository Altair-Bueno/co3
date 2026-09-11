//! Structures and macros related to FFI and generation of FFI bindings. Any type that implements
//! [`ExternC`] can be used in the FFI bindings generated with [`ffi!`]. It is advisable
//! to implement [`RustSpec`](rust_spec::RustSpec) and benefit from automatic
//! implementation of [`ExternC`].
//!
//! ```rust
//! # mod ffi {
//! # use co3::ffi;
//!
//! #[cfg(not(feature = "ffi-extern"))]
//! struct Local(u8);
//!
//! #[cfg(not(feature = "ffi-extern"))]
//! fn take_local_ref(a: &Local) {
//!     unimplemented!()
//! }
//!
//! ffi! {
//!     #![cfg_attr(not(feature = "ffi-extern"), unsafe(export("C")))]
//!     #![cfg_attr(feature = "ffi-extern", unsafe(extern("C")))]
//!
//!     #![symbol_prefix = "provider"]
//!
//!     type Local;
//!
//!     fn take_local_ref(a: &Local);
//! }
//! # }
//! ```
#![cfg_attr(feature = "allocator-api", feature(allocator_api))]
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;
extern crate self as co3;

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};

#[cfg(feature = "derive")]
pub use co3_derive::*;
use disjoint_impls::disjoint_impls;
#[doc(hidden)]
pub use rust_spec;
use rust_spec::{
    One, RustSpec,
    mutability::{Exclusive, Interior},
    niche::{NicheStabilityKind, WithNiche, WithoutNiche},
    size::{ExternTypeLike, MetaSized, SizedKind, SliceLike, Zero},
};
#[cfg(feature = "alloc")]
use rust_spec::{Stable, Unstable, size::MetadataKind};
// TODO: I don't like having to reexport macros from other crates
#[doc(hidden)]
pub use impls::impls;

#[cfg(feature = "alloc")]
use crate::boxed::{CBox, CBoxedSlice};
use crate::{
    option::ReprCOption,
    result::ReprCResult,
    slice::{CSlice, CSliceMut},
    stored::{DecodeOwned, EmptyStore, EncodeOwned, Store},
    wide::Wide,
};

pub mod borrow;
#[cfg(feature = "alloc")]
pub mod boxed;
pub mod cell;
#[doc(hidden)]
pub mod either;
pub mod niche;
pub mod option;
mod primitives;
pub mod result;
pub mod slice;
mod std_impls;
pub mod stored;
pub mod tag;
pub mod transmute;
pub mod tuple;
pub mod wide;

trait Thin {}
impl<K: SizedKind> Thin for rust_spec::size::Sized<K> {}
impl Thin for ExternTypeLike {}

#[cfg(feature = "alloc")]
trait Dst {}
#[cfg(feature = "alloc")]
impl Dst for ExternTypeLike {}
#[cfg(feature = "alloc")]
impl<K: MetadataKind> Dst for MetaSized<K> {}

pub trait Error {
    fn trap_value() -> Self;
    fn unknown_tag() -> Self;
    fn soft_sync_error() -> Self;
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

unsafe impl<T: CFnArg> CFnReturn for T {}

disjoint_impls! {
    /// A Rust type that has an `extern "C"` ABI
    pub trait ExternC {
        /// The C-compatible representation of this Rust type.
        type CType: ReprC + ?Sized;
    }

    impl<R: ExternC + ?Sized> ExternC for &R
    where
        R: RustSpec<Size: Thin, Mutability = Exclusive>,
    {
        type CType = *const R::CType;
    }
    impl<R: ExternC + ?Sized> ExternC for &R
    where
        R: RustSpec<Size: Thin, Mutability = Interior>,
    {
        type CType = *mut R::CType;
    }
    impl<R: Wide<Data: ExternC, Metadata = usize> + ?Sized> ExternC for &R
    where
        R: RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type CType = CSlice<<R::Data as ExternC>::CType>;
    }
    impl<R: Wide<Data: ExternC, Metadata = usize> + ?Sized> ExternC for &R
    where
        R: RustSpec<Size = MetaSized<SliceLike>, Mutability = Interior>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type CType = CSliceMut<<R::Data as ExternC>::CType>;
    }

    impl<R: ExternC + ?Sized> ExternC for &mut R
    where
        R: RustSpec<Size: Thin>,
    {
        type CType = *mut R::CType;
    }
    impl<R: Wide<Data: ExternC, Metadata = usize> + ?Sized> ExternC for &mut R
    where
        R: RustSpec<Size = MetaSized<SliceLike>>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type CType = CSliceMut<<R::Data as ExternC>::CType>;
    }

    #[cfg(feature = "alloc")]
    impl<R: ExternC, S: SizedKind> ExternC for Box<R>
    where
        R: RustSpec<Size = rust_spec::size::Sized<S>>,
        <R as ExternC>::CType: Sized,
    {
        type CType = CBox<R::CType>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized> ExternC for Box<R>
    where
        R: RustSpec<Size = MetaSized<SliceLike>>,
        R: Wide<Data: ExternC, Metadata = usize>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type CType = CBoxedSlice<<R::Data as ExternC>::CType>;
    }

    impl<R: ExternC> ExternC for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
        <R as ExternC>::CType: Sized,
    {
        type CType = ReprCOption<R::CType>;
    }
    impl<R: ExternC, N: NicheStabilityKind> ExternC for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
    {
        type CType = R::CType;
    }

    // TODO: The next 3 impls are the same
    impl<R: ExternC, E: ExternC, K: SizedKind> ExternC for Result<R, E>
    where
        R: RustSpec<Size = rust_spec::size::Sized<K>>,
        E: RustSpec<Size = rust_spec::size::Sized<K>>,
        // TODO: There is an error in disjoint_impls! that doesn't allow to use
        // `Extern<CType: Copy>` constraint. Fix that! Check other Result sites
        <R as ExternC>::CType: Copy,
        <E as ExternC>::CType: Copy,
    {
        type CType = ReprCResult<R::CType, E::CType>;
    }
    impl<R: ExternC, E: ExternC, N: NicheStabilityKind> ExternC for Result<R, E>
    where
        R: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<
            Size = rust_spec::size::Sized<Zero>,
            Alignment = rust_spec::Gt<rust_spec::One>,
        >,
        <R as ExternC>::CType: Copy,
        <E as ExternC>::CType: Copy,
    {
        type CType = ReprCResult<R::CType, E::CType>;
    }
    impl<R: ExternC, E: ExternC, N: NicheStabilityKind> ExternC for Result<R, E>
    where
        R: RustSpec<
            Size = rust_spec::size::Sized<Zero>,
            Alignment = rust_spec::Gt<rust_spec::One>,
        >,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>, Niche = WithNiche<N>>,
        <R as ExternC>::CType: Copy,
        <E as ExternC>::CType: Copy,
    {
        type CType = ReprCResult<R::CType, E::CType>;
    }
    impl<R: ExternC, E, N: NicheStabilityKind> ExternC for Result<R, E>
    where
        R: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = rust_spec::size::Sized<Zero>, Alignment = One>,
    {
        type CType = R::CType;
    }
    impl<R, E: ExternC, N: NicheStabilityKind> ExternC for Result<R, E>
    where
        R: RustSpec<Size = rust_spec::size::Sized<Zero>, Alignment = One>,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>, Niche = WithNiche<N>>,
    {
        type CType = E::CType;
    }
}

disjoint_impls! {
    /// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
    pub trait Encode: EncodeOwned {}

    #[cfg(feature = "alloc")]
    impl<R: ?Sized> Encode for Box<R>
    where
        Self: RustSpec<Layout = Stable> + EncodeOwned,
    {}
    #[cfg(feature = "alloc")]
    impl<R: RustSpec<Layout = Stable> + ?Sized> Encode for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + EncodeOwned,
    {}
}

disjoint_impls! {
    /// Facilitates conversion into a Rust type from a corresponding C-compatible representation.
    pub trait Decode<'d>: DecodeOwned<'d> {}

    #[cfg(feature = "alloc")]
    impl<'d, R: ?Sized> Decode<'d> for Box<R>
    where
        Self: RustSpec<Layout = Stable>,
        Self: DecodeOwned<'d>,
    {}
    #[cfg(feature = "alloc")]
    impl<'d, R: ?Sized> Decode<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable>,
        Self: DecodeOwned<'d>,
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

/// Perform the conversion from `T` into [`T::CType`] using external storage.
///
/// Prefer using [`encode`] whenever possible
pub fn soft_encode<T: Encode>(item: T, store: &mut T::Store) -> T::CType {
    item.soft_encode(store)
}

/// Perform the conversion from `T` into [`T::CType`].
pub fn encode<T: Encode<Store: EmptyStore>>(item: T) -> T::CType {
    stored::encode_owned(item)
}

/// Perform the conversion from [`T::CType`](ExternC::CType) into `T` using external storage.
///
/// Prefer using [`decode`] whenever possible
///
/// # Safety
///
/// - All conversions from a pointer must ensure pointer validity beforehand
pub unsafe fn soft_decode<'d, T: Decode<'d>>(
    source: T::CType,
    store: &'d mut T::Store,
) -> Option<T> {
    unsafe { T::soft_decode(source, store) }
}

/// Perform the conversion from [`T::CType`](ExternC::CType) into `T`.
///
/// # Safety
///
/// - All conversions from a pointer must ensure pointer validity beforehand
pub unsafe fn decode<'d, T: Decode<'d, Store: EmptyStore> + 'd>(source: T::CType) -> Option<T> {
    unsafe { stored::decode_owned(source) }
}

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
            let encoded = soft_encode(value_mut_ref, &mut *store);
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
            let encoded = soft_encode(ref_mut, &mut *store);
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
            let decoded = unsafe { soft_decode::<&mut _>(c_ptr, &mut *store) }.unwrap();

            *decoded = Some(new_val);
            store.sync().unwrap();
        }
        assert_eq!(c_opt, ReprCOption::Some(42u8));

        let mut c_opts = [ReprCOption::Some(1u8)];
        let c_slice = CSliceMut::from_slice(&mut c_opts);
        let x: u8 = 10;
        {
            let mut store = Box::default();
            let decoded = unsafe { soft_decode::<&mut [_]>(c_slice, &mut *store) }.unwrap();

            decoded[0] = Some(x);
            store.sync().unwrap();
        }
        assert_eq!(c_opts[0], ReprCOption::Some(10u8));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn encode_stored_ref_mut_slice() {
        use tuple::ReprCTuple2;

        let mut tuples = [(1_u32, true)];
        let slice_ref: &mut [_] = &mut tuples;

        {
            let mut store = Box::default();

            let encoded = soft_encode(slice_ref, &mut store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };

            c_slice[0] = ReprCTuple2(100, 1);
            store.sync().unwrap();
        }

        assert_eq!(tuples[0].0, 100);
    }

    #[test]
    #[cfg(feature = "alloc")]
    // FIXME: This test demonstrates that ownership of &mut Box<(u32,)> is leaked
    // from one side to the other: https://github.com/mversic/co3/issues/182
    fn encode_stored_mut_box_allows_pointer_replacement() {
        use boxed::CBox;
        use tuple::ReprCTuple2;

        let mut value = Box::new((1_u32, true));

        {
            let mut store = Box::default();
            let encoded = soft_encode(&mut value, &mut *store);
            let original_data = unsafe { (*encoded).data };
            let replacement = CBox::from_box(Box::new(ReprCTuple2(100, 1)));
            let replacement_data = replacement.data;

            unsafe {
                *encoded = replacement;
            }

            assert_ne!(original_data, replacement_data);
            store.sync().unwrap();
        }

        assert_eq!(*value, (100, true));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn decode_stored_ref_mut_slice() {
        use tuple::ReprCTuple2;

        let mut tuples = [ReprCTuple2(10, 1)];
        let c_slice = CSliceMut::from_slice(&mut tuples);

        {
            let mut store = Box::default();
            let decoded =
                unsafe { soft_decode::<&mut [(u32, bool)]>(c_slice, &mut store) }.unwrap();

            decoded[0].0 = 100;
            store.sync().unwrap();
        }

        assert_eq!(tuples[0].0, 100);
    }
}
