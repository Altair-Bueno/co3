//! Utilities for defining opaque pointer handles and shared handle logic.

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
/// The handle declarations must be wrapped in `unsafe { ... }` because the caller
/// must guarantee that each generated handle ID is unique within the relevant
/// FFI dispatch domain and matches the declarations on the other side of the
/// boundary.
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
///     unsafe {
///         Foo1,
///         Foo2 = 8,
///         Bar1,
///     }
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
    ( unsafe { $($decls:tt)* } ) => {
        $crate::handles! { @next 0; $($decls)* }
    };

    ( @next $next:expr; ) => {};
    ( @next $next:expr; , $($rest:tt)* ) => {
        $crate::handles! { @next $next; $($rest)* }
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
        compile_error!("handle declarations require `unsafe { ... }`. Check safety section of `co3::Handle`");
    };
}
