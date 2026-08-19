//! Utilities for opaque pointer handles required for tagged dispatch.
use crate::Encode;

/// Groups handles that use the same tag type.
pub trait HandleFamily {
    // TODO: Should Copy be required?
    type Kind: Encode + Copy;
}

/// Represents an opaque handle in an FFI context
///
/// # Safety
///
/// If two structures implement the same id, it may result in a void pointer cast to a wrong type
pub unsafe trait Handle: HandleFamily {
    /// Unique identifier of the handle. Most commonly, it is
    /// used to facilitate generic monomorphization over FFI
    const ID: Self::Kind;
}
