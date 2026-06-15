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
    ReprC,
    niche::{NicheFamily, WithCustomNiche, WithStableNiche, WithoutNiche},
    size::{MetaSized, SizeFamily, Thin},
};

/// Marker for a type that don't have a guaranteed representation and requires explicit conversion.
pub enum ReprRust {}

/// Marker for a type that is transmuted to another type and thus delegates its conversion.
pub struct Transmuted<K>(core::marker::PhantomData<K>, Infallible);

/// Marker for a robust [`ReprC`] type that does not require conversion
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
        /// - If `Self` is [`ReprC`], set [`ReprFamily::Kind`] to [`Transmuted<Robust>`].
        ///   The type is passed to FFI functions as-is, without conversion.
        ///
        /// - If [`ReprFamily::Kind`] is [`Transmuted<NonRobust>`] the type has the same
        ///   representation as the target type but may still require validation.
        ///
        /// - If [`ReprFamily::Kind`] is [`ReprRust`], `Self` must provide hand-written impl of [`crate::ExternC`].
        ///   References to [`ReprRust`] types are `soft` and conversion of the value will make use of the store.
        type Kind;
    }

    // FIXME: The following 2 impls should be compressible into 1, but disjoint_impls can't do it?
    // and then further compressed so only 3 impls remain in total
    impl<R: ReprFamily<Kind = Transmuted<Robust>> + SizeFamily<Kind: Thin> + ?Sized> ReprFamily for &R {
        type Kind = Transmuted<NonRobust>;
    }
    impl<R: ReprFamily<Kind = Transmuted<NonRobust>> + SizeFamily<Kind: Thin> + ?Sized> ReprFamily for &R {
        type Kind = Transmuted<NonRobust>;
    }
    impl<R: ReprFamily<Kind = Transmuted<Robust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for &R {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = Transmuted<NonRobust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for &R {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = ReprRust> + ?Sized> ReprFamily for &R {
        type Kind = ReprRust;
    }

    impl<R: ReprFamily<Kind = Transmuted<Robust>> + SizeFamily<Kind: Thin> + ?Sized> ReprFamily for &mut R {
        type Kind = Transmuted<NonRobust>;
    }
    impl<R: ReprFamily<Kind = Transmuted<NonRobust>> + SizeFamily<Kind: Thin> + ?Sized> ReprFamily for &mut R {
        type Kind = Transmuted<NonRobust>;
    }
    impl<R: ReprFamily<Kind = Transmuted<Robust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for &mut R {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = Transmuted<NonRobust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for &mut R {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = ReprRust> + ?Sized> ReprFamily for &mut R {
        type Kind = ReprRust;
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted<Robust>> + SizeFamily<Kind = crate::size::Sized>> ReprFamily for Box<R> {
        type Kind = Transmuted<NonRobust>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted<NonRobust>> + SizeFamily<Kind = crate::size::Sized>> ReprFamily for Box<R> {
        type Kind = Transmuted<NonRobust>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted<Robust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for Box<R> {
        type Kind = ReprRust;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted<NonRobust>> + SizeFamily<Kind = MetaSized<U>> + ?Sized, U> ReprFamily for Box<R> {
        type Kind = ReprRust;
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + ?Sized> ReprFamily for Box<R> {
        type Kind = ReprRust;
    }

    // TODO: impls here can also be compressed
    impl<R: ReprFamily<Kind = Transmuted<Robust>>> ReprFamily for Option<R> {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = Transmuted<NonRobust>> + NicheFamily<Kind = WithoutNiche>> ReprFamily for Option<R> {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = Transmuted<NonRobust>> + NicheFamily<Kind = WithCustomNiche>> ReprFamily for Option<R> {
        type Kind = ReprRust;
    }
    impl<R: ReprFamily<Kind = Transmuted<NonRobust>> + NicheFamily<Kind = WithStableNiche>> ReprFamily for Option<R> {
        // TODO: Sometimes it should be mapped to Robust when R has 1 niche
        // https://github.com/mversic/co3/issues/33
        type Kind = Transmuted<NonRobust>;
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

// FIXME: This bound may be useless by now
disjoint_impls! {
    // TODO: IMO this bound should be ReprFamily<Kind = Transmuted<K>>
    // This can be a case in point to split Robustness out of ReprFamily
    pub trait EncodeReprFamily: ReprFamily {
        type Kind;
    }

    //impl<R: ReprFamily<Kind = Robust> + ?Sized> EncodeReprFamily for R {
    //    type Kind = Robust;
    //}
    //impl<R: ReprFamily<Kind = ReprRust> + ?Sized> EncodeReprFamily for R {
    //    type Kind = ReprRust;
    //}

    impl<R: EncodeReprFamily, K> EncodeReprFamily for [R]
    where
        Self: ReprFamily<Kind = Transmuted<K>>,
    {
        type Kind = <R as EncodeReprFamily>::Kind;
    }

    impl<R: EncodeReprFamily + ?Sized, K> EncodeReprFamily for &R
    where
        Self: ReprFamily<Kind = Transmuted<K>>,
    {
        type Kind = <R as EncodeReprFamily>::Kind;
    }

    // NOTE: `ReprC` is doing real work here because it is an `unsafe` trait and guarantees
    // soundness of every mutation of the pointee from the other side of the `FFI` boundary
    impl<R: ReprFamily<Kind = Transmuted<Robust>> + ReprC + ?Sized> EncodeReprFamily for &mut R
    where
        Self: ReprFamily<Kind = Transmuted<Robust>>,
    {
        type Kind = Transmuted<Robust>;
    }
    impl<R: ReprFamily<Kind = Transmuted<NonRobust>> + ?Sized> EncodeReprFamily for &mut R
    where
        Self: ReprFamily<Kind = Transmuted<NonRobust>>,
    {
        // TODO: With a `Unchecked<T>` wrapper we can transmute this
        type Kind = ReprRust;
    }

    // FIXME: &UnsafeCell should be distinguished as a mutable reference type
    // NOTE: `ReprC` is doing real work here because it is an `unsafe` trait and guarantees
    // soundness of every mutation of the pointee from the other side of the `FFI` boundary
    //impl<R: ReprFamily<Kind = Robust> + ReprC + ?Sized> EncodeReprFamily for &UnsafeCell<R>
    //where
    //    Self: ReprFamily<Kind = Transmuted>,
    //{
    //    type Kind = Transmuted;
    //}
    //impl<R: NicheFamily<Kind = Transmuted> + ?Sized> EncodeReprFamily for &UnsafeCell<R>
    //where
    //    Self: ReprFamily<Kind = Transmuted>,
    //{
    //    // TODO: With a `Unchecked<T>` wrapper we can transmute this
    //    type Kind = ReprRust;
    //}

    #[cfg(feature = "alloc")]
    impl<R: EncodeReprFamily + ?Sized, K> EncodeReprFamily for Box<R>
    where
        Self: ReprFamily<Kind = Transmuted<K>>,
    {
        type Kind = <R as EncodeReprFamily>::Kind;
    }

    impl<R: EncodeReprFamily, const N: usize, K> EncodeReprFamily for [R; N]
    where
        Self: ReprFamily<Kind = Transmuted<K>>,
    {
        type Kind = <R as EncodeReprFamily>::Kind;
    }

    impl<R: EncodeReprFamily, K> EncodeReprFamily for Option<R>
    where
        Self: ReprFamily<Kind = Transmuted<K>>,
    {
        type Kind = <R as EncodeReprFamily>::Kind;
    }
}

#[cfg(feature = "alloc")]
impl<R> ReprFamily for Vec<R> {
    type Kind = ReprRust;
}

impl<K> Add<ReprRust> for Transmuted<K> {
    type Output = ReprRust;

    fn add(self, _: ReprRust) -> Self::Output {
        unreachable!()
    }
}

impl<K> Add<Transmuted<K>> for ReprRust {
    type Output = ReprRust;

    fn add(self, _: Transmuted<K>) -> Self::Output {
        unreachable!()
    }
}

impl Add for ReprRust {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

impl Add for Transmuted<NonRobust> {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

impl Add for Transmuted<Robust> {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}

impl Add<Transmuted<NonRobust>> for Transmuted<Robust> {
    type Output = Transmuted<NonRobust>;

    fn add(self, _: Transmuted<NonRobust>) -> Self::Output {
        unreachable!()
    }
}

impl Add<Transmuted<Robust>> for Transmuted<NonRobust> {
    type Output = Self;

    fn add(self, _: Transmuted<Robust>) -> Self::Output {
        unreachable!()
    }
}
