//! Logic related to the conversion of primitives to and from FFI-compatible representation

use crate::{
    niche::{NicheFamily, WithCustomNiche, WithoutNiche},
    repr::{NonRobust, ReprC, ReprFamily, Robust},
    size::{MetaSized, SizeFamily, SliceLike},
};

macro_rules! primitive_derive {
    ( $primitive:ty ) => {
        impl ReprFamily for $primitive {
            type Kind = ReprC<Robust>;
        }
        unsafe impl SizeFamily for $primitive {
            type Kind = crate::size::Sized<crate::size::NonZst>;
        }
        impl NicheFamily for $primitive {
            type Kind = WithoutNiche;
        }
    };
}

macro_rules! raw_pointer_derive {
    ( $mutability:tt ) => {
        impl<R: ReprFamily + ?Sized> ReprFamily for *$mutability R {
            type Kind = R::Kind;
        }
        unsafe impl<R: ?Sized> SizeFamily for *$mutability R {
            type Kind = crate::size::Sized<crate::size::NonZst>;
        }
        impl<R: ?Sized> NicheFamily for *$mutability R {
            type Kind = WithoutNiche;
        }
    };
}

// FIXME:
macro_rules! impl_fn_types {
    ( $( ( $( $arg:ident ),* ) ),* $(,)? ) => {$(
        impl<$($arg: ReprFamily,)* R> ReprFamily for extern "C" fn($($arg),*) -> R {
            type Kind = ();
        }
        //impl<$($arg,)* R> SizeFamily for extern "C" fn($($arg),*) -> R {
        //    type Kind = crate::size::Sized<crate::size::NonZst>;
        //}
        //impl<$($arg,)* R> NicheFamily for extern "C" fn($($arg),*) -> R {
        //    type Kind = WithStableNiche;
        //}

        )*
    }
}

macro_rules! fieldless_enum_derive {
    ( $src:ty => $dst:ty: {$niche_val:expr}: $validity_fn:expr ) => {
        impl ReprFamily for $src {
            type Kind = ReprC<NonRobust>;
        }
        unsafe impl SizeFamily for $src {
            type Kind = crate::size::Sized<crate::size::NonZst>;
        }
        impl NicheFamily for $src {
            type Kind = WithCustomNiche;
        }
    };
}

primitive_derive! { usize }
primitive_derive! { isize }
primitive_derive! { u8 }
primitive_derive! { i8 }
primitive_derive! { u16 }
primitive_derive! { i16 }
primitive_derive! { u32 }
primitive_derive! { i32 }
primitive_derive! { u64 }
primitive_derive! { i64 }
primitive_derive! { u128 }
primitive_derive! { i128 }
primitive_derive! { f32 }
primitive_derive! { f64 }

raw_pointer_derive! { const }
raw_pointer_derive! { mut }

impl<R: ReprFamily> ReprFamily for [R] {
    type Kind = R::Kind;
}
unsafe impl<R> SizeFamily for [R] {
    type Kind = MetaSized<SliceLike>;
}

impl<R: ReprFamily, const N: usize> ReprFamily for [R; N] {
    type Kind = R::Kind;
}
unsafe impl<R: SizeFamily, const N: usize> SizeFamily for [R; N] {
    type Kind = R::Kind;
}

impl_fn_types! {
    (),
    (A),
    (A, B),
    (A, B, C),
    (A, B, C, D),
    (A, B, C, D, E),
    (A, B, C, D, E, F),
    (A, B, C, D, E, F, G),
    (A, B, C, D, E, F, G, H),
    (A, B, C, D, E, F, G, H, I),
    (A, B, C, D, E, F, G, H, I, J),
    (A, B, C, D, E, F, G, H, I, J, K),
    (A, B, C, D, E, F, G, H, I, J, K, L),
}

fieldless_enum_derive! {
    char => u32: {0x110000}:
    |i: &u32| char::from_u32(*i).is_some()
}
fieldless_enum_derive! {
    bool => u8: {2}:
    |i: &u8| *i == 0 || *i == 1
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, vec::Vec};
    use static_assertions::assert_impl_all;

    use super::*;
    use crate::{
        niche::WithStableNiche,
        repr::{ReprRust, Robust},
    };

    #[test]
    fn robust_u8() {
        assert_impl_all!(u8:
            ReprFamily<Kind = ReprC<Robust>>,
            NicheFamily<Kind = WithoutNiche>,
        );
        assert_impl_all!(&u8:
            ReprFamily<Kind = ReprC<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&mut u8:
            ReprFamily<Kind = ReprC<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<u8>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&[u8]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!(&mut [u8]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[u8]>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<u8>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!([u8; 2]:
            ReprFamily<Kind = ReprC<Robust>>,
            NicheFamily<Kind = WithoutNiche>,
        );
        assert_impl_all!(Option<u8>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
        );
    }
}
