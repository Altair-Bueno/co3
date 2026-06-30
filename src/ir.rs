//! Internal Representation (IR) of Rust types during conversion into FFI types.
//!
//! While you can implement [`crate::ExternC`] directly on your type, it is often
//! preferable to map it into IR by implementing [`Ir`]. This approach gives you
//! automatic, correct, and zero-cost conversions from IR to the equivalent C type.
#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use core::{convert::Infallible, ops::Add};

use disjoint_impls::disjoint_impls;

use crate::{
    niche::{NicheFamily, WithCustomNiche, WithStableNiche, WithoutNiche},
    size::{MetaSized, SizeFamily, Thin},
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

    // FIXME: The following 2 impls should be compressible into 1, but disjoint_impls can't do it?
    // and then further compressed so only 3 impls remain in total
    impl<R: ReprFamily<Kind = ReprC<Robust>> + SizeFamily<Kind: Thin> + ?Sized> ReprFamily for &R {
        type Kind = ReprC<NonRobust>;
    }
    impl<R: ReprFamily<Kind = ReprC<NonRobust>> + SizeFamily<Kind: Thin> + ?Sized> ReprFamily for &R {
        type Kind = ReprC<NonRobust>;
    }
    impl<R: ReprFamily<Kind = ReprC<Robust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for &R {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = ReprC<NonRobust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for &R {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = ReprRust> + ?Sized> ReprFamily for &R {
        type Kind = ReprRust;
    }

    impl<R: ReprFamily<Kind = ReprC<Robust>> + SizeFamily<Kind: Thin> + ?Sized> ReprFamily for &mut R {
        type Kind = ReprC<NonRobust>;
    }
    impl<R: ReprFamily<Kind = ReprC<NonRobust>> + SizeFamily<Kind: Thin> + ?Sized> ReprFamily for &mut R {
        type Kind = ReprC<NonRobust>;
    }
    impl<R: ReprFamily<Kind = ReprC<Robust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for &mut R {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = ReprC<NonRobust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for &mut R {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = ReprRust> + ?Sized> ReprFamily for &mut R {
        type Kind = ReprRust;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprC<Robust>> + SizeFamily<Kind = crate::size::Sized>> ReprFamily for Box<R> {
        type Kind = ReprC<NonRobust>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprC<NonRobust>> + SizeFamily<Kind = crate::size::Sized>> ReprFamily for Box<R> {
        type Kind = ReprC<NonRobust>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprC<Robust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for Box<R> {
        type Kind = ReprRust;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprC<NonRobust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for Box<R> {
        type Kind = ReprRust;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + ?Sized> ReprFamily for Box<R> {
        type Kind = ReprRust;
    }

    // TODO: impls here can also be compressed
    impl<R: ReprFamily<Kind = ReprC<Robust>>> ReprFamily for Option<R> {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = ReprC<NonRobust>> + NicheFamily<Kind = WithoutNiche>> ReprFamily for Option<R> {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = ReprC<NonRobust>> + NicheFamily<Kind = WithCustomNiche>> ReprFamily for Option<R> {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = ReprC<NonRobust>> + NicheFamily<Kind = WithStableNiche>> ReprFamily for Option<R> {
        // TODO: Sometimes it should be mapped to Robust when R has 1 niche
        // https://github.com/mversic/co3/issues/33
        type Kind = ReprC<NonRobust>;
    }
    impl<R: ReprFamily<Kind = ReprRust>> ReprFamily for Option<R> {
        type Kind = ReprRust;
    }
    // TODO: Make a test. I think the example type is &Box<u8>
    // Should it become Stored in this case? Likewise for Result
    //impl<R: ReprFamily<Kind = ReprRust> + NicheFamily<Kind = WithStableNiche>> ReprFamily for Option<R> {
    //    type Kind = ReprRust;
    //}

    impl<R: NicheFamily<Kind = WithoutNiche>, E: NicheFamily<Kind = WithoutNiche>> ReprFamily for Result<R, E> {
        type Kind = ReprRust;
    }
    // TODO: Implement for niche optimized Results
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
