//! Utilities for defining opaque pointer handles and shared handle logic.
#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use core::ffi::c_void;

use crate::Encode;

pub trait HandleFamily {
    // TODO: Should Copy be required?
    type Kind: Encode + Copy;
}

/// Represents an opaque handle in an FFI context
///
/// # Safety
///
/// If two structures implement the same id, it may result in a void pointer cast to a wrong type
///
/// Prefer [`crate::handles!`] for assigning IDs.
pub unsafe trait Handle: HandleFamily {
    /// Unique identifier of the handle. Most commonly, it is
    /// used to facilitate generic monomorphization over FFI
    const ID: Self::Kind;
}

/// Implements [`Handle`] for a list of types.
///
/// ID assignment follows Rust fieldless enum discriminant rules:
/// - first entry without `= ...` gets `0`
/// - each following implicit entry gets previous ID + 1
/// - explicit `= ...` resets the running value for following entries
///
/// # Example
///
/// ```rust
/// use co3::handles;
///
/// struct Foo1;
/// struct Foo2;
/// struct Bar1;
///
/// # impl co3::handle::HandleFamily for Foo1 {
/// #     type Kind = u8;
/// # }
///
/// # impl co3::handle::HandleFamily for Foo2 {
/// #     type Kind = u8;
/// # }
///
/// # impl co3::handle::HandleFamily for Bar1 {
/// #     type Kind = u8;
/// # }
///
/// handles! {
///     Foo1,
///     Foo2 = 8,
///     Bar1,
/// }
///
/// /* will produce:
/// impl Handle for Foo1 {
///     const ID: Id = 0;
/// }
/// impl Handle for Foo2 {
///     const ID: Id = 8;
/// }
/// impl Handle for Bar1 {
///     const ID: Id = 9;
/// } */
/// ```
#[macro_export]
macro_rules! handles {
    ( @next $next:expr; ) => {};
    ( @next $next:expr; , $($rest:tt)* ) => {
        $crate::handles! { @next $next; $($rest)* }
    };

    ( @next $next:expr; for<$($lt:lifetime),+> $ty:ty = $id:expr, $($rest:tt)* ) => {
        unsafe impl<$($lt),+> $crate::handle::Handle for $ty {
            const ID: Self::Kind = $id;
        }

        $crate::handles! { @next ($id) + 1; $($rest)* }
    };
    ( @next $next:expr; for<$($lt:lifetime),+> $ty:ty = $id:expr $(,)? ) => {
        unsafe impl<$($lt),+> $crate::handle::Handle for $ty {
            const ID: Self::Kind = $id;
        }
    };

    ( @next $next:expr; for<$($lt:lifetime),+> $ty:ty, $($rest:tt)* ) => {
        unsafe impl<$($lt),+> $crate::handle::Handle for $ty {
            const ID: Self::Kind = $next;
        }

        $crate::handles! { @next ($next) + 1; $($rest)* }
    };
    ( @next $next:expr; for<$($lt:lifetime),+> $ty:ty $(,)? ) => {
        unsafe impl<$($lt),+> $crate::handle::Handle for $ty {
            const ID: Self::Kind = $next;
        }
    };

    ( @next $next:expr; $ty:ty = $id:expr, $($rest:tt)* ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: Self::Kind = $id;
        }

        $crate::handles! { @next ($id) + 1; $($rest)* }
    };
    ( @next $next:expr; $ty:ty = $id:expr $(,)? ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: Self::Kind = $id;
        }
    };

    ( @next $next:expr; $ty:ty, $($rest:tt)* ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: Self::Kind = $next;
        }

        $crate::handles! { @next ($next) + 1; $($rest)* }
    };
    ( @next $next:expr; $ty:ty $(,)? ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: Self::Kind = $next;
        }
    };

    ( $($decls:tt)* ) => {
        $crate::handles! { @next 0; $($decls)* }
    };
}

// FIXME: Make both Self and Self::Erased `Sized`
// It makes little sense to allow ?Sized to erase but it's too bothersome change for me atm
pub unsafe trait Erase {
    type Erased: ?Sized;
}

unsafe impl<'a, T: Erase + ?Sized> Erase for &'a T {
    type Erased = &'a T::Erased;
}

unsafe impl<'a, T: Erase + ?Sized> Erase for &'a mut T {
    type Erased = &'a mut T::Erased;
}

#[cfg(feature = "alloc")]
unsafe impl<T: Erase + ?Sized> Erase for Box<T> {
    type Erased = Box<T::Erased>;
}

unsafe impl Erase for c_void {
    type Erased = Self;
}
unsafe impl Erase for str {
    type Erased = Self;
}
unsafe impl<T: Erase<Erased: Sized>> Erase for [T] {
    type Erased = [T::Erased];
}

unsafe impl<T: Erase> Erase for core::marker::PhantomData<T> {
    type Erased = core::marker::PhantomData<T::Erased>;
}

#[cfg(feature = "alloc")]
unsafe impl<T: Erase<Erased: Sized>> Erase for Vec<T> {
    type Erased = Vec<T::Erased>;
}

unsafe impl<T: Erase<Erased: Sized>> Erase for Option<T> {
    type Erased = Option<T::Erased>;
}

unsafe impl<T: Erase<Erased: Sized>, E: Erase<Erased: Sized>> Erase for Result<T, E> {
    type Erased = Result<T::Erased, E::Erased>;
}
