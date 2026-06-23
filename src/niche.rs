//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

use core::{ops::Add, ptr::NonNull};

#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::boxed::{CBox, CBoxedSlice};
use crate::{
    ExternC, ReprC, assert_arr_has_non_zero_len,
    option::ReprCOption,
    result::ReprCResult,
    size::{MetaSized, SizeFamily, Thin},
    slice::{CSlice, CSliceMut},
};

/// Marker trait for an [`NicheFamily`] type of a Rust type that has a niche value (stable or custom)
///
/// There are only 2 notable implementations of this trait:
/// 1. [`Transmuted`] types have a single stable (compiler guaranteed) niche value (e.g. `&u32`)
/// 2. [`Stored`] types have a custom defined (by this crate) niche value (e.g. `[NonZeroU32; 2]`)
pub(crate) trait WithNiche {}

/// Marker for a type that has a single stable (compiler guaranteed) niche value (e.g. `&u32`).
///
/// Only a handful of [`crate::transmute::Transmuted`] types have a stable niche
pub enum WithStableNiche {}

/// Marker for a type that has a custom defined (by this crate) niche (e.g. `[NonZeroU8; 2]`).
pub enum WithCustomNiche {}

/// Marker for a type that has no trap representations and therefore no niche value
pub enum WithoutNiche {}

disjoint_impls! {
    /// Type that has a trap representation that can be used as a niche value.
    ///
    /// # Example
    ///
    /// [`Option<bool>`]     - will be serilized into one byte
    /// [`Option<*const T>`] - will take the size of the pointer
    pub trait Niche: ExternC<CType: Copy> + Sized {
        const NICHE_VALUE: Self::CType;
    }

    impl<R, C> Niche for &R
    where
        Self: ExternC<CType = *const C>,
    {
        const NICHE_VALUE: Self::CType = core::ptr::null();
    }
    impl<R: ?Sized, C> Niche for &R
    where
        Self: ExternC<CType = CSlice<C>>,
    {
        const NICHE_VALUE: Self::CType = CSlice::NICHE_VALUE;
    }

    impl<R, C> Niche for &mut R
    where
        Self: ExternC<CType = *mut C>,
    {
        const NICHE_VALUE: Self::CType = core::ptr::null_mut();
    }
    impl<R: ?Sized, C> Niche for &mut R
    where
        Self: ExternC<CType = CSliceMut<C>>,
    {
        const NICHE_VALUE: Self::CType = CSliceMut::NICHE_VALUE;
    }

    #[cfg(feature = "alloc")]
    impl<R, C> Niche for Box<R>
    where
        Self: ExternC<CType = CBox<C>>,
    {
        const NICHE_VALUE: Self::CType = CBox::NICHE_VALUE;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C> Niche for Box<R>
    where
        Self: ExternC<CType = CBoxedSlice<C>>,
    {
        const NICHE_VALUE: Self::CType = CBoxedSlice::NICHE_VALUE;
    }

    impl<T, C> Niche for NonNull<T>
    where
        Self: ExternC<CType = *mut C>,
    {
        const NICHE_VALUE: Self::CType = core::ptr::null_mut();
    }
    // TODO: Support ?Sized
    //impl<R: ?C> Niche for NonNull<R>
    //where
    //    Self: ExternC<CType = CBoxedSlice<C>>,
    //{
    //    const NICHE_VALUE: Self::CType = CBoxedSlice::none();
    //}
}

/// Type that has a compiler guaranteed [`Niche`] value (e.g. `Box<T>`)
///
/// The stable niche value is made use of when serializing [`Option<T>`].
///
/// # Safety
///
/// - the niche value must be congruent with what is guaranteed by the Rust compiler
pub unsafe trait StableNiche: Niche {}

disjoint_impls! {
    /// Niche kind of the type in the internal representation [IR](`crate::ir::Repr`)
    pub trait NicheFamily: Sized {
        /// The internal representation (i.e. type family) of the type
        ///
        /// - If `Self` doesn't have any niche value, set [`NicheFamily::Kind`] to [`WithoutNiche`].
        ///   `Option<T>` will be serialized as [`crate::option::ReprCOption`]
        ///
        /// - If `Self` has a compiler guaranteed niche value, set [`NicheFamily::Kind`] to [`WithStableNiche`].
        ///   `Option<T>` will be blindly transmuted into the underlying [`ReprC`] type
        ///
        /// - Otherwise, if `Self` has at least one trap, set [`NicheFamily::Kind`] to [`WithCustomNiche`].
        ///   `Option<T>` will be serialized into a [`T::CType`] with a manually set niche value
        type Kind;
    }

    impl<R: SizeFamily<Kind = MetaSized<K>> + ?Sized, K> NicheFamily for &R {
        type Kind = WithCustomNiche;
    }
    impl<R: SizeFamily<Kind: Thin> + ?Sized> NicheFamily for &R {
        type Kind = WithStableNiche;
    }

    impl<R: SizeFamily<Kind = MetaSized<K>> + ?Sized, K> NicheFamily for &mut R {
        type Kind = WithCustomNiche;
    }
    impl<R: SizeFamily<Kind: Thin> + ?Sized> NicheFamily for &mut R {
        type Kind = WithStableNiche;
    }

    #[cfg(feature = "alloc")]
    impl<R: SizeFamily<Kind = MetaSized<K>> + ?Sized, K> NicheFamily for Box<R> {
        type Kind = WithCustomNiche;
    }
    #[cfg(feature = "alloc")]
    impl<R: SizeFamily<Kind = crate::size::Sized>> NicheFamily for Box<R> {
        type Kind = WithStableNiche;
    }

    impl<R: NicheFamily<Kind = WithoutNiche>, const N: usize> NicheFamily for [R; N] {
        type Kind = WithoutNiche;
    }
    impl<R: NicheFamily<Kind: WithNiche>, const N: usize> NicheFamily for [R; N] {
        type Kind = WithCustomNiche;
    }

    impl<R: NicheFamily<Kind = WithoutNiche>> NicheFamily for Option<R> {
        type Kind = WithCustomNiche;
    }
    impl<R: NicheFamily<Kind = WithStableNiche>> NicheFamily for Option<R> {
        type Kind = WithoutNiche;
    }
    // TODO: IMHO compiler should be able to resolve circular dependencies here, but it doesn't work for now so I've bounded previous with Niche
    // This issue could be of some help: https://github.com/mversic/co3/issues/33. This seems to be a limitation of the compiler known as
    // circular/cyclic resolution or (co)inductive cycle. The case shown here creates a cycle but only one solution is possible afaik
    //impl<R: NicheFamily<Kind = WithCustomNiche>> NicheFamily for Option<R> where Option<Self>: ReprFamily<Kind: ReprRustOrTransmutedNonRobust> {
    //    type Kind = WithCustomNiche;
    //}
    //impl<R: NicheFamily<Kind = WithCustomNiche>> NicheFamily for Option<R> where Option<Self>: ReprFamily<Kind = ReprRust> {
    //    type Kind = WithoutNiche;
    //}
    impl<R: NicheFamily<Kind = WithCustomNiche>> NicheFamily for Option<R> where Self: Niche {
        type Kind = WithCustomNiche;
    }

    impl<R: NicheFamily<Kind = WithoutNiche>, E: NicheFamily<Kind = WithoutNiche>> NicheFamily for Result<R, E> {
        type Kind = WithCustomNiche;
    }
    // TODO: Implement for niche optimized Results
}

#[cfg(feature = "alloc")]
impl<R> NicheFamily for Vec<R> {
    type Kind = WithCustomNiche;
}

#[cfg(feature = "alloc")]
impl<R, C> Niche for Vec<R>
where
    Self: ExternC<CType = CBoxedSlice<C>>,
{
    const NICHE_VALUE: Self::CType = CBoxedSlice::NICHE_VALUE;
}

impl<R: Niche, const N: usize> Niche for [R; N]
where
    Self: ExternC<CType = [R::CType; N]>,
{
    const NICHE_VALUE: Self::CType = {
        assert_arr_has_non_zero_len::<N>();
        [R::NICHE_VALUE; N]
    };
}

impl<R, C: ReprC + Copy> Niche for Option<R>
where
    Self: ExternC<CType = ReprCOption<C>>,
{
    const NICHE_VALUE: Self::CType = ReprCOption::NICHE_VALUE;
}
impl<T: ExternC<CType: Copy>, E: ExternC<CType: Copy>> Niche for Result<T, E>
where
    Self: ExternC<CType = ReprCResult<T::CType, E::CType>>,
    T: NicheFamily<Kind = crate::niche::WithoutNiche>,
    E: NicheFamily<Kind = crate::niche::WithoutNiche>,
{
    const NICHE_VALUE: Self::CType = ReprCResult::NICHE_VALUE;
}

// TODO: Depends on: https://github.com/mversic/co3/issues/33
impl Niche for Option<bool> {
    const NICHE_VALUE: Self::CType = 3;
}
impl Niche for Option<Option<bool>> {
    const NICHE_VALUE: Self::CType = 4;
}

unsafe impl<R: ?Sized> StableNiche for &R where Self: Niche {}
unsafe impl<R: ?Sized> StableNiche for &mut R where Self: Niche {}
#[cfg(feature = "alloc")]
unsafe impl<R: ?Sized> StableNiche for Box<R> where Self: Niche {}
unsafe impl<R: ?Sized> StableNiche for NonNull<R> where Self: Niche {}

impl WithNiche for WithStableNiche {}
impl WithNiche for WithCustomNiche {}

impl Add for WithoutNiche {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}
impl<T: WithNiche> Add<T> for WithoutNiche {
    type Output = WithCustomNiche;

    fn add(self, _: T) -> Self::Output {
        unreachable!()
    }
}
impl Add<WithoutNiche> for WithCustomNiche {
    type Output = Self;

    fn add(self, _: WithoutNiche) -> Self::Output {
        unreachable!()
    }
}
impl Add<WithoutNiche> for WithStableNiche {
    type Output = WithCustomNiche;

    fn add(self, _: WithoutNiche) -> Self::Output {
        unreachable!()
    }
}
impl Add<WithCustomNiche> for WithStableNiche {
    type Output = WithCustomNiche;

    fn add(self, _: WithCustomNiche) -> Self::Output {
        unreachable!()
    }
}
impl Add for WithStableNiche {
    type Output = WithCustomNiche;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}
impl Add<WithStableNiche> for WithCustomNiche {
    type Output = Self;

    fn add(self, _: WithStableNiche) -> Self::Output {
        unreachable!()
    }
}
impl Add for WithCustomNiche {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc_crate::string::String;
    use core::{mem::ManuallyDrop, num::NonZero};

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::{
        Decode, Encode, ReprC,
        ir::{ReprFamily, ReprRust},
        slice::CSlice,
        stored::SoftEncodeOwned,
        tuple::ReprCTuple2,
    };

    #[test]
    fn nested_option_niche_family() {
        assert_impl_all!(Option<bool>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = u8>,
            Decode<'static>,
            Encode,

        );
        assert_impl_all!(Option<Option<bool>>:
            NicheFamily<Kind = WithCustomNiche>,
            ReprFamily<Kind = ReprRust>,
            Niche<CType = u8>,
            Decode<'static>,
            Encode,

        );
        assert_impl_all!(Option<(u8, NonZero<u8>)>:
            ReprFamily<Kind = ReprRust>,
            // TODO: Depends on: https://github.com/mversic/co3/issues/33
            //NicheFamily<Kind = WithoutNiche>,
            //Niche<CType = ReprCTuple2<u8, u8>>,
            ExternC<CType = ReprCTuple2<u8, u8>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(Option<bool>: ReprC);
        assert_not_impl_any!(Option<Option<bool>>: ReprC);
    }

    #[test]
    fn niche_values() {
        assert_eq!(core::ptr::null::<u8>(), None::<&bool>.encode());
        assert_eq!(
            core::ptr::null::<u8>(),
            None::<&mut bool>.soft_encode(&mut Default::default())
        );

        #[cfg(feature = "alloc")]
        assert_eq!(CBoxedSlice::<u8>::NICHE_VALUE, None::<String>.encode());
        #[cfg(feature = "alloc")]
        assert_eq!(CBoxedSlice::<u8>::NICHE_VALUE, None::<Box<str>>.encode());

        assert_eq!(CSlice::<u8>::NICHE_VALUE, None::<&str>.encode());

        #[cfg(feature = "alloc")]
        assert_eq!(
            co3::slice::CSliceMut::<u8>::NICHE_VALUE,
            None::<&mut str>.soft_encode(&mut Default::default())
        );

        assert_eq!(core::ptr::null_mut(), None::<NonNull<u32>>.encode());

        #[cfg(feature = "alloc")]
        assert_eq!(
            CBoxedSlice::<u8>::NICHE_VALUE,
            None::<ManuallyDrop<String>>.encode()
        );

        assert_eq!(2_u8, None::<ManuallyDrop<bool>>.encode());
    }
}
