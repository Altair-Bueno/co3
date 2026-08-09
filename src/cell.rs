//! Raw access to a value stored behind interior mutability.

use core::cell::{Cell, UnsafeCell};

/// Provides a raw mutable pointer to a value that supports interior mutation.
///
/// Calling [`Self::get`] does not grant permission to dereference the returned
/// pointer. Callers must still uphold Rust's aliasing and any type-specific
/// borrowing rules.
pub trait InteriorMut {
    /// The value accessible through this interior-mutable container.
    type Target: ?Sized;

    /// Returns a raw mutable pointer to [`Self::Target`].
    fn get(&self) -> *mut Self::Target;
}

impl<T: ?Sized> InteriorMut for UnsafeCell<T> {
    type Target = T;

    #[inline]
    fn get(&self) -> *mut Self::Target {
        UnsafeCell::get(self)
    }
}

impl<T: ?Sized> InteriorMut for Cell<T> {
    type Target = T;

    #[inline]
    fn get(&self) -> *mut Self::Target {
        Cell::as_ptr(self)
    }
}

impl<R: InteriorMut + ?Sized> InteriorMut for &R {
    type Target = R::Target;

    #[inline]
    fn get(&self) -> *mut Self::Target {
        InteriorMut::get(*self)
    }
}

impl<R: InteriorMut + ?Sized> InteriorMut for &mut R {
    type Target = R::Target;

    #[inline]
    fn get(&self) -> *mut Self::Target {
        InteriorMut::get(&**self)
    }
}

#[cfg(feature = "alloc")]
impl<R: InteriorMut + ?Sized> InteriorMut for alloc::boxed::Box<R> {
    type Target = R::Target;

    #[inline]
    fn get(&self) -> *mut Self::Target {
        InteriorMut::get(self.as_ref())
    }
}

#[cfg(test)]
#[cfg(all(feature = "alloc", feature = "derive"))]
mod tests {
    use alloc::boxed::Box;
    use core::cell::UnsafeCell;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::{Encode, ReprC, rust_spec::RustSpec};

    #[derive(RustSpec, ReprC)]
    #[repr(C)]
    struct DerivedInteriorMut {
        value: UnsafeCell<u8>,
    }

    #[derive(RustSpec, ReprC)]
    #[repr(C)]
    struct MultipleFields {
        first: UnsafeCell<u8>,
        second: u8,
    }

    #[derive(RustSpec, ReprC)]
    enum SingleVariant {
        Value(UnsafeCell<u8>),
    }

    #[derive(RustSpec, ReprC)]
    #[repr(u8)]
    enum TaggedSingleVariant {
        Value(UnsafeCell<u8>),
    }

    #[test]
    fn repr_c_derive_exposes_interior_mutable_address() {
        assert_impl_all!(DerivedInteriorMut: InteriorMut);
        assert_impl_all!(SingleVariant: InteriorMut);
        assert_not_impl_any!(TaggedSingleVariant: InteriorMut);
        assert_impl_all!(&DerivedInteriorMut: Encode);
        assert_not_impl_any!(MultipleFields: InteriorMut);

        let value = DerivedInteriorMut {
            value: UnsafeCell::new(7),
        };
        let pointer = InteriorMut::get(&value);

        assert_eq!(pointer, value.value.get());

        let value = SingleVariant::Value(UnsafeCell::new(11));
        let pointer = InteriorMut::get(&value);
        let SingleVariant::Value(field) = &value;

        assert_eq!(pointer, field.get());
    }
}
