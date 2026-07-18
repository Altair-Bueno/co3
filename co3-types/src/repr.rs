//! Internal Representation (IR) of Rust types during conversion into FFI types.
//!
//! While you can implement [`crate::ExternC`] directly on your type, it is often
//! preferable to map it into IR by implementing [`Ir`]. This approach gives you
//! automatic, correct, and zero-cost conversions from IR to the equivalent C type.
#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::{convert::Infallible, ops::Add};

use disjoint_impls::disjoint_impls;

use crate::{
    niche::{NicheFamily, WithCustomNiche, WithStableNiche, WithoutNiche},
    size::{MetaSized, NonZst, SizeFamily, Thin, Zst},
};

/// Marker for a type that don't have a guaranteed representation and requires explicit conversion.
pub enum Unstable {}

/// Marker for a type that is transmuted to another type and thus delegates its conversion.
pub struct Stable<K>(core::marker::PhantomData<K>, Infallible);

/// Marker for a robust [`crate::ReprC`] type that does not require conversion
pub enum Robust {}

/// Marker for a non-robust type that is still transmuted by the ABI layer.
pub enum NonRobust {}

disjoint_impls! {
    /// Type that can be converted to and from an internal representation (IR).
    ///
    /// Predefined IR types automatically implement [`crate::ExternC`] and related conversion traits.
    pub trait ReprFamily {
        /// The internal representation (i.e. type family) of the type
        ///
        /// - If `Self` is [`crate::ReprC`], set [`ReprFamily::Kind`] to [`Stable<Robust>`].
        ///   The type is passed to FFI functions as-is, without conversion.
        ///
        /// - If [`ReprFamily::Kind`] is [`Stable<NonRobust>`] the type has the same
        ///   representation as the target [`crate::ExternC::CType`] but requires validation.
        ///
        /// - If [`ReprFamily::Kind`] is [`Unstable`], `Self` must provide hand-written impl of [`crate::ExternC`].
        ///   References to [`Unstable`] types are `soft` and conversion of the value will make use of the store.
        type Kind;
    }

    impl<R: ReprFamily<Kind = Unstable> + ?Sized> ReprFamily for &R {
        type Kind = Unstable;
    }
    impl<R: ReprFamily<Kind = Stable<K>> + SizeFamily<Kind: Thin> + ?Sized, K> ReprFamily for &R {
        type Kind = Stable<NonRobust>;
    }
    impl<R: ReprFamily<Kind = Stable<K>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U, K> ReprFamily for &R {
        type Kind = Unstable;
    }

    impl<R: ReprFamily<Kind = Unstable> + ?Sized> ReprFamily for &mut R {
        type Kind = Unstable;
    }
    impl<R: ReprFamily<Kind = Stable<K>> + SizeFamily<Kind: Thin> + ?Sized, K> ReprFamily for &mut R {
        type Kind = Stable<NonRobust>;
    }
    impl<R: ReprFamily<Kind = Stable<K>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U, K> ReprFamily for &mut R {
        type Kind = Unstable;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Unstable> + ?Sized> ReprFamily for Box<R> {
        type Kind = Unstable;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Stable<K>> + SizeFamily<Kind = crate::size::Sized<S>>, S, K> ReprFamily for Box<R> {
        type Kind = Stable<NonRobust>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Stable<K>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U, K> ReprFamily for Box<R> {
        type Kind = Unstable;
    }

    impl<R: NicheFamily<Kind = WithoutNiche>> ReprFamily for Option<R> {
        type Kind = Unstable;
    }
    impl<R: NicheFamily<Kind = WithCustomNiche>> ReprFamily for Option<R> {
        type Kind = Unstable;
    }
    impl<R: NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = Unstable>> ReprFamily for Option<R> {
        type Kind = Unstable;
    }
    impl<R: NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = Stable<NonRobust>>> ReprFamily for Option<R> {
        // FIXME: Sometimes it should be mapped to Robust when R has 1 niche. Also for Result
        // https://github.com/mversic/co3/issues/33
        type Kind = Stable<NonRobust>;
    }

    impl<
        R: SizeFamily<Kind = crate::size::Sized<K>>,
        E: SizeFamily<Kind = crate::size::Sized<K>>,
        K
    > ReprFamily for Result<R, E> {
        type Kind = Unstable;
    }

    impl<
        R: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithoutNiche>,
        E: SizeFamily<Kind = crate::size::Sized<Zst>>,
    > ReprFamily for Result<R, E> {
        type Kind = Unstable;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithCustomNiche>,
        E: SizeFamily<Kind = crate::size::Sized<Zst>>,
    > ReprFamily for Result<R, E> {
        type Kind = Unstable;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = Unstable>,
        E: SizeFamily<Kind = crate::size::Sized<Zst>>,
    > ReprFamily for Result<R, E> {
        type Kind = Unstable;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = Stable<NonRobust>>,
        E: SizeFamily<Kind = crate::size::Sized<Zst>>,
    > ReprFamily for Result<R, E> {
        type Kind = Stable<NonRobust>;
    }

    impl<
        R: SizeFamily<Kind = crate::size::Sized<Zst>>,
        E: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithoutNiche>,
    > ReprFamily for Result<R, E> {
        type Kind = Unstable;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<Zst>>,
        E: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithCustomNiche>,
    > ReprFamily for Result<R, E> {
        type Kind = Unstable;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<Zst>>,
        E: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = Unstable>,
    > ReprFamily for Result<R, E> {
        type Kind = Unstable;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<Zst>>,
        E: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = Stable<NonRobust>>,
    > ReprFamily for Result<R, E> {
        type Kind = Stable<NonRobust>;
    }
}

#[cfg(feature = "alloc")]
impl<R> ReprFamily for Vec<R> {
    type Kind = Unstable;
}

impl<K> Add<Unstable> for Stable<K> {
    type Output = Unstable;

    fn add(self, _: Unstable) -> Self::Output {
        unreachable!()
    }
}

impl<K> Add<Stable<K>> for Unstable {
    type Output = Unstable;

    fn add(self, _: Stable<K>) -> Self::Output {
        unreachable!()
    }
}

impl Add for Unstable {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

impl Add for Stable<NonRobust> {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

impl Add for Stable<Robust> {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

impl Add<Stable<NonRobust>> for Stable<Robust> {
    type Output = Stable<NonRobust>;

    fn add(self, _: Stable<NonRobust>) -> Self::Output {
        unreachable!()
    }
}

impl Add<Stable<Robust>> for Stable<NonRobust> {
    type Output = Self;

    fn add(self, _: Stable<Robust>) -> Self::Output {
        unreachable!()
    }
}
