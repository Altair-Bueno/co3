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
pub enum ReprRust {}

/// Marker for a type that is transmuted to another type and thus delegates its conversion.
pub struct ReprC<K>(core::marker::PhantomData<K>, Infallible);

/// Marker for a robust [`crate::RobustReprC`] type that does not require conversion
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
        /// - If `Self` is [`crate::RobustReprC`], set [`ReprFamily::Kind`] to [`ReprC<Robust>`].
        ///   The type is passed to FFI functions as-is, without conversion.
        ///
        /// - If [`ReprFamily::Kind`] is [`ReprC<NonRobust>`] the type has the same
        ///   representation as the target [`crate::ExternC::CType`] but requires validation.
        ///
        /// - If [`ReprFamily::Kind`] is [`ReprRust`], `Self` must provide hand-written impl of [`crate::ExternC`].
        ///   References to [`ReprRust`] types are `soft` and conversion of the value will make use of the store.
        type Kind;
    }

    impl<R: ReprFamily<Kind = ReprRust> + ?Sized> ReprFamily for &R {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = ReprC<K>> + SizeFamily<Kind: Thin> + ?Sized, K> ReprFamily for &R {
        type Kind = ReprC<NonRobust>;
    }
    impl<R: ReprFamily<Kind = ReprC<K>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U, K> ReprFamily for &R {
        type Kind = ReprRust;
    }

    impl<R: ReprFamily<Kind = ReprRust> + ?Sized> ReprFamily for &mut R {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = ReprC<K>> + SizeFamily<Kind: Thin> + ?Sized, K> ReprFamily for &mut R {
        type Kind = ReprC<NonRobust>;
    }
    impl<R: ReprFamily<Kind = ReprC<K>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U, K> ReprFamily for &mut R {
        type Kind = ReprRust;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + ?Sized> ReprFamily for Box<R> {
        type Kind = ReprRust;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprC<K>> + SizeFamily<Kind = crate::size::Sized<S>>, S, K> ReprFamily for Box<R> {
        type Kind = ReprC<NonRobust>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprC<K>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U, K> ReprFamily for Box<R> {
        type Kind = ReprRust;
    }

    impl<R: NicheFamily<Kind = WithoutNiche>> ReprFamily for Option<R> {
        type Kind = ReprRust;
    }
    impl<R: NicheFamily<Kind = WithCustomNiche>> ReprFamily for Option<R> {
        type Kind = ReprRust;
    }
    impl<R: NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = ReprRust>> ReprFamily for Option<R> {
        type Kind = ReprRust;
    }
    impl<R: NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = ReprC<NonRobust>>> ReprFamily for Option<R> {
        // FIXME: Sometimes it should be mapped to Robust when R has 1 niche. Also for Result
        // https://github.com/mversic/co3/issues/33
        type Kind = ReprC<NonRobust>;
    }

    impl<
        R: SizeFamily<Kind = crate::size::Sized<K>>,
        E: SizeFamily<Kind = crate::size::Sized<K>>,
        K
    > ReprFamily for Result<R, E> {
        type Kind = ReprRust;
    }

    impl<
        R: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithoutNiche>,
        E: SizeFamily<Kind = crate::size::Sized<Zst>>,
    > ReprFamily for Result<R, E> {
        type Kind = ReprRust;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithCustomNiche>,
        E: SizeFamily<Kind = crate::size::Sized<Zst>>,
    > ReprFamily for Result<R, E> {
        type Kind = ReprRust;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = ReprRust>,
        E: SizeFamily<Kind = crate::size::Sized<Zst>>,
    > ReprFamily for Result<R, E> {
        type Kind = ReprRust;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = ReprC<NonRobust>>,
        E: SizeFamily<Kind = crate::size::Sized<Zst>>,
    > ReprFamily for Result<R, E> {
        type Kind = ReprC<NonRobust>;
    }

    impl<
        R: SizeFamily<Kind = crate::size::Sized<Zst>>,
        E: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithoutNiche>,
    > ReprFamily for Result<R, E> {
        type Kind = ReprRust;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<Zst>>,
        E: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithCustomNiche>,
    > ReprFamily for Result<R, E> {
        type Kind = ReprRust;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<Zst>>,
        E: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = ReprRust>,
    > ReprFamily for Result<R, E> {
        type Kind = ReprRust;
    }
    impl<
        R: SizeFamily<Kind = crate::size::Sized<Zst>>,
        E: SizeFamily<Kind = crate::size::Sized<NonZst>> + NicheFamily<Kind = WithStableNiche> + ReprFamily<Kind = ReprC<NonRobust>>,
    > ReprFamily for Result<R, E> {
        type Kind = ReprC<NonRobust>;
    }
}

#[cfg(feature = "alloc")]
impl<R> ReprFamily for Vec<R> {
    type Kind = ReprRust;
}

impl<K> Add<ReprRust> for ReprC<K> {
    type Output = ReprRust;

    fn add(self, _: ReprRust) -> Self::Output {
        unreachable!()
    }
}

impl<K> Add<ReprC<K>> for ReprRust {
    type Output = ReprRust;

    fn add(self, _: ReprC<K>) -> Self::Output {
        unreachable!()
    }
}

impl Add for ReprRust {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

impl Add for ReprC<NonRobust> {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

impl Add for ReprC<Robust> {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

impl Add<ReprC<NonRobust>> for ReprC<Robust> {
    type Output = ReprC<NonRobust>;

    fn add(self, _: ReprC<NonRobust>) -> Self::Output {
        unreachable!()
    }
}

impl Add<ReprC<Robust>> for ReprC<NonRobust> {
    type Output = Self;

    fn add(self, _: ReprC<Robust>) -> Self::Output {
        unreachable!()
    }
}
