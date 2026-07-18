//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

use core::ops::Add;

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;

use crate::size::{MetaSized, NonZst, SizeFamily, Thin, Zst};

/// Marker trait for an [`NicheFamily`] type of a Rust type that has a niche value (stable or custom)
///
/// There are only 2 notable implementations of this trait:
/// 1. [`ReprC`] types have a single stable (compiler guaranteed) niche value (e.g. `&u32`)
/// 2. [`Stored`] types have a custom defined (by this crate) niche value (e.g. `[NonZeroU32; 2]`)
pub(crate) trait WithNiche {}

/// Marker for a type that has a single stable (compiler guaranteed) niche value (e.g. `&u32`).
///
/// Only a handful of [`crate::ir::ReprC`] types have a stable niche
pub enum WithStableNiche {}

/// Marker for a type that has a custom defined (by this crate) niche (e.g. `[NonZeroU8; 2]`).
pub enum WithCustomNiche {}

/// Marker for a type that has no trap representations and therefore no niche value
pub enum WithoutNiche {}

disjoint_impls! {
    /// Niche kind of the type in the internal representation [IR](`crate::ir::Repr`)
    pub trait NicheFamily {
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

    impl<R: NicheFamily<Kind = WithoutNiche>> NicheFamily for [R] {
        type Kind = WithoutNiche;
    }
    impl<R: NicheFamily<Kind: WithNiche>> NicheFamily for [R] {
        type Kind = WithCustomNiche;
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
    impl<R: SizeFamily<Kind = crate::size::Sized<S>>, S> NicheFamily for Box<R> {
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
    //impl<R: NicheFamily<Kind = WithCustomNiche>> NicheFamily for Option<R> where Option<Self>: ReprFamily<Kind = Unstable> {
    //    type Kind = WithoutNiche;
    //}
    //impl<R: NicheFamily<Kind = WithCustomNiche>> NicheFamily for Option<R> where Self: Niche {
    //    type Kind = WithCustomNiche;
    //}

    impl<
        R: NicheFamily<Kind = WithoutNiche>,
        E: NicheFamily<Kind = WithoutNiche>,
    > NicheFamily for Result<R, E> {
        type Kind = WithCustomNiche;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithStableNiche>,
        E: SizeFamily<Kind = crate::size::Sized<Zst>> + NicheFamily,
    > NicheFamily for Result<R, E> {
        type Kind = WithoutNiche;
    }
    // FIXME:
    //impl<
    //    R: SizeFamily<Kind = crate::size::Sized<Zst>> + NicheFamily,
    //    E: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithStableNiche>,
    //> NicheFamily for Result<R, E> {
    //    type Kind = WithoutNiche;
    //}
    // TODO: Implement for all niche optimized Results
}

#[cfg(feature = "alloc")]
impl<R> NicheFamily for Vec<R> {
    type Kind = WithCustomNiche;
}

impl NicheFamily for Option<bool> {
    type Kind = WithCustomNiche;
}
impl NicheFamily for Option<Option<bool>> {
    type Kind = WithCustomNiche;
}

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
    use core::num::NonZero;

    use static_assertions::assert_impl_all;

    use super::*;
    use crate::repr::{ReprFamily, Unstable};

    #[test]
    fn nested_option_niche_family() {
        assert_impl_all!(Option<bool>:
            ReprFamily<Kind = Unstable>,
            NicheFamily<Kind = WithCustomNiche>,
        );

        assert_impl_all!(Option<Option<bool>>:
            NicheFamily<Kind = WithCustomNiche>,
            ReprFamily<Kind = Unstable>,
        );

        assert_impl_all!(Option<(u8, NonZero<u8>)>:
            ReprFamily<Kind = Unstable>,
            // TODO: Depends on: https://github.com/mversic/co3/issues/33
            //NicheFamily<Kind = WithoutNiche>,
            //Niche<CType = ReprCTuple2<u8, u8>>,
        );
    }
}
