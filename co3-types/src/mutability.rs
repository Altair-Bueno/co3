//! Mutability classification of Rust types.

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::ops::Add;

/// Marker for types whose reachable state may be mutated through shared access.
pub enum Interior {}

/// Marker for types whose reachable state requires exclusive access to mutate.
pub enum Exclusive {}

/// Classifies whether a Rust type has reachable interior mutability.
pub trait MutabilityFamily {
    /// Mutability classification marker for this type.
    ///
    /// - Set to [`Interior`] if shared access may mutate reachable state.
    /// - Set to [`Exclusive`] if mutation requires exclusive access.
    type Kind;
}

impl<R: MutabilityFamily + ?Sized> MutabilityFamily for &R {
    type Kind = R::Kind;
}

impl<R: MutabilityFamily + ?Sized> MutabilityFamily for &mut R {
    type Kind = R::Kind;
}

#[cfg(feature = "alloc")]
impl<R: MutabilityFamily + ?Sized> MutabilityFamily for Box<R> {
    type Kind = R::Kind;
}

#[cfg(feature = "alloc")]
impl<R: MutabilityFamily> MutabilityFamily for Vec<R> {
    type Kind = R::Kind;
}

impl<R: MutabilityFamily> MutabilityFamily for Option<R> {
    type Kind = R::Kind;
}

impl<R: MutabilityFamily, E: MutabilityFamily> MutabilityFamily for Result<R, E>
where
    R::Kind: Add<E::Kind>,
{
    type Kind = <R::Kind as Add<E::Kind>>::Output;
}

impl<K> Add<K> for Exclusive {
    type Output = K;

    fn add(self, _: K) -> Self::Output {
        unreachable!()
    }
}

impl<K> Add<K> for Interior {
    type Output = Self;

    fn add(self, _: K) -> Self::Output {
        unreachable!()
    }
}
