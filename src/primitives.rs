//! Logic related to the conversion of primitives to and from FFI-compatible representation

use crate::{
    CFnArg, CFnReturn, Decode, ExternC, ReprC, SoftDecode, SoftEncode,
    borrow::{Borrow, BorrowCast, ToOwned},
    handle::Erase,
    ir::{NonRobust, ReprFamily, Robust, Transmuted},
    niche::{Niche, NicheFamily, WithCustomNiche, WithoutNiche},
    size::{MetaSized, SizeFamily, SliceLike},
    stored::{SoftDecodeOwned, SoftEncodeOwned},
    transmute::CheckedTransmute,
};

macro_rules! primitive_derive {
    ( $primitive:ty ) => {
        impl ReprFamily for $primitive {
            type Kind = Transmuted<Robust>;
        }
        impl SizeFamily for $primitive {
            type Kind = crate::size::Sized;
        }
        impl NicheFamily for $primitive {
            type Kind = WithoutNiche;
        }

        impl Borrow for $primitive {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Owner = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'itm> ToOwned<'itm> for $primitive {
            #[inline(always)]
            fn to_owned(source: Self) -> Self {
                source
            }
        }

        impl ExternC for $primitive {
            type CType = Self;
        }
        impl SoftEncodeOwned for $primitive {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'d> SoftDecodeOwned<'d> for $primitive {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl SoftEncode for $primitive {}
        impl SoftDecode<'_> for $primitive {}

        unsafe impl CheckedTransmute for $primitive {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl Erase for $primitive {
            type Erased = Self;
        }

        unsafe impl ReprC for $primitive {}
        unsafe impl CFnArg for $primitive {}
        unsafe impl BorrowCast for $primitive {
            type AsConst = Self;
            type AsMut = Self;
        }
    };
}

macro_rules! raw_pointer_derive {
    ( $mutability:tt ) => {
        impl<R: ReprFamily + ?Sized> ReprFamily for *$mutability R {
            type Kind = R::Kind;
        }
        impl<R: ?Sized> SizeFamily for *$mutability R {
            type Kind = crate::size::Sized;
        }
        impl<R: ?Sized> NicheFamily for *$mutability R {
            type Kind = WithoutNiche;
        }

        impl<R: ?Sized> Borrow for *$mutability R {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Owner = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'itm, R: ?Sized> ToOwned<'itm> for *$mutability R {
            #[inline(always)]
            fn to_owned(source: Self) -> Self {
                source
            }
        }

        impl<R: ReprC + ?Sized> ExternC for *$mutability R {
            type CType = Self;
        }
        impl<R: ReprC + ?Sized> SoftEncodeOwned for *$mutability R {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'d, R: ReprC + ?Sized> SoftDecodeOwned<'d> for *$mutability R {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl<R: ReprC + ?Sized> SoftEncode for *$mutability R {}
        impl<R: ReprC + ?Sized> SoftDecode<'_> for *$mutability R {}

        unsafe impl<R: ReprC + ?Sized> CheckedTransmute for *$mutability R {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<R: Erase + ?Sized> Erase for *$mutability R {
            type Erased = *$mutability R::Erased;
        }

        unsafe impl<R: ReprC + ?Sized> ReprC for *$mutability R {}
        unsafe impl<R: ReprC + ?Sized> CFnArg for *$mutability R {}
        unsafe impl<R: ReprC + ?Sized> BorrowCast for *$mutability R {
            type AsConst = Self;
            type AsMut = Self;
        }
    };
}

// FIXME:
macro_rules! impl_fn_types {
    ( $( ( $( $arg:ident ),* ) ),* $(,)? ) => {$(
        impl<$($arg: ReprFamily + CFnArg,)* R: CFnReturn> ReprFamily for extern "C" fn($($arg),*) -> R {
            type Kind = ();
        }
        //impl<$($arg,)* R> SizeFamily for extern "C" fn($($arg),*) -> R {
        //    type Kind = crate::size::Sized;
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
            type Kind = Transmuted<NonRobust>;
        }
        impl SizeFamily for $src {
            type Kind = crate::size::Sized;
        }
        impl NicheFamily for $src {
            type Kind = WithCustomNiche;
        }

        unsafe impl CheckedTransmute for $src {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                $validity_fn(target)
            }
        }

        impl Borrow for $src {
            type Borrowed<'itm> = Self;

            type Owner = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm> {
                self
            }
        }
        impl<'itm> ToOwned<'itm> for $src {
            #[inline(always)]
            fn to_owned(source: Self::Borrowed<'itm>) -> Self {
                source
            }
        }

        impl ExternC for $src {
            type CType = $dst;
        }
        impl SoftEncodeOwned for $src {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self as $dst
            }
        }
        impl<'d> SoftDecodeOwned<'d> for $src {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                unsafe { <$dst>::decode(source)? }.try_into().ok()
            }
        }

        impl SoftEncode for $src {}
        impl SoftDecode<'_> for $src {}

        impl Niche for $src {
            const NICHE_VALUE: Self::CType = $niche_val;
        }

        unsafe impl Erase for $src {
            type Erased = $src;
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
impl<R> SizeFamily for [R] {
    type Kind = MetaSized<SliceLike>;
}

unsafe impl<R: ReprC> ReprC for [R] {}

impl<R: ReprC> ExternC for [R] {
    type CType = Self;
}

impl<R: ReprFamily, const N: usize> ReprFamily for [R; N] {
    type Kind = R::Kind;
}
impl<T, const N: usize> SizeFamily for [T; N] {
    type Kind = crate::size::Sized;
}

unsafe impl<R: ReprC, const N: usize> ReprC for [R; N] {}
impl<R: ExternC<CType: Sized>, const N: usize> ExternC for [R; N] {
    type CType = [R::CType; N];
}

unsafe impl<T: Erase<Erased: Sized>, const N: usize> Erase for [T; N] {
    type Erased = [T::Erased; N];
}
unsafe impl<R: BorrowCast, const N: usize> BorrowCast for [R; N] {
    type AsConst = [R::AsConst; N];
    type AsMut = [R::AsMut; N];
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
    use alloc_crate::{boxed::Box, vec::Vec};
    use static_assertions::assert_impl_all;

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::{CBox, CBoxedSlice};
    use crate::{
        Encode,
        ir::{ReprRust, Robust},
        niche::{StableNiche, WithStableNiche},
        option::ReprCOption,
        slice::{CSlice, CSliceMut},
    };

    #[test]
    fn robust_u8() {
        assert_impl_all!(u8:
            ReprFamily<Kind = Transmuted<Robust>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = u8>,
            Decode<'static>,
            Encode,
            ReprC,
        );
        assert_impl_all!(&u8:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut u8:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<u8>:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[u8]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [u8]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[u8]>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<u8>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([u8; 2]:
            ReprFamily<Kind = Transmuted<Robust>>,
            NicheFamily<Kind = WithoutNiche>,
            Decode<'static>,
            Encode,
            ReprC,
        );
        assert_impl_all!(Option<u8>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = ReprCOption<u8>>,
            Decode<'static>,
            Encode,
        );
    }
}
