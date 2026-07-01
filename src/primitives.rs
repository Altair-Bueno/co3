//! Logic related to the conversion of primitives to and from FFI-compatible representation

use crate::{
    CFnArg, CFnReturn, Decode, Encode, ExternC, RobustReprC, assert_arr_has_non_zero_len,
    borrow::{Borrow, BorrowCast, BorrowCastMut, ToOwned},
    ir::{NonRobust, ReprC, ReprFamily, Robust},
    niche::{Niche, NicheFamily, WithCustomNiche, WithoutNiche},
    size::{MetaSized, SizeFamily, SliceLike},
    stored::{ArrayStore, DecodeOwned, EmptyStore, EncodeOwned},
    transmute::CheckedTransmute,
};

macro_rules! primitive_derive {
    ( $primitive:ty ) => {
        impl ReprFamily for $primitive {
            type Kind = ReprC<Robust>;
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
        impl EncodeOwned for $primitive {
            type Store = ();

            #[inline(always)]
            fn soft_encode_owned<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'d> DecodeOwned<'d> for $primitive {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode_owned<'itm: 'd>(
                source: Self::CType,
                (): &mut (),
            ) -> Option<Self> {
                Some(source)
            }
        }

        impl Encode for $primitive {}
        impl Decode<'_> for $primitive {}

        unsafe impl CheckedTransmute for $primitive {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl RobustReprC for $primitive {}
        unsafe impl CFnArg for $primitive {}

        unsafe impl BorrowCast for $primitive {
            type AsConst = Self;
        }
        unsafe impl BorrowCastMut for $primitive {
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

        impl<R: RobustReprC + ?Sized> ExternC for *$mutability R {
            type CType = Self;
        }
        impl<R: RobustReprC + ?Sized> EncodeOwned for *$mutability R {
            type Store = ();

            #[inline(always)]
            fn soft_encode_owned<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'d, R: RobustReprC + ?Sized> DecodeOwned<'d> for *$mutability R {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode_owned<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl<R: RobustReprC + ?Sized> Encode for *$mutability R {}
        impl<R: RobustReprC + ?Sized> Decode<'_> for *$mutability R {}

        unsafe impl<R: RobustReprC + ?Sized> CheckedTransmute for *$mutability R {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<R: RobustReprC + ?Sized> RobustReprC for *$mutability R {}
        unsafe impl<R: RobustReprC + ?Sized> CFnArg for *$mutability R {}

        unsafe impl<R: RobustReprC + ?Sized> BorrowCast for *$mutability R {
            type AsConst = Self;
        }
        unsafe impl<R: RobustReprC + ?Sized> BorrowCastMut for *$mutability R {
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
            type Kind = ReprC<NonRobust>;
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
        impl EncodeOwned for $src {
            type Store = ();

            #[inline(always)]
            fn soft_encode_owned<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self as $dst
            }
        }
        impl<'d> DecodeOwned<'d> for $src {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode_owned<'itm: 'd>(
                source: Self::CType,
                (): &mut (),
            ) -> Option<Self> {
                unsafe { <$dst as DecodeOwned>::decode_owned(source)? }
                    .try_into()
                    .ok()
            }
        }

        impl Encode for $src {}
        impl Decode<'_> for $src {}

        impl Niche for $src {
            const NICHE_VALUE: Self::CType = $niche_val;
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

unsafe impl<R: RobustReprC> RobustReprC for [R] {}

impl<R: RobustReprC> ExternC for [R] {
    type CType = Self;
}
unsafe impl<R: BorrowCast<AsConst: Copy>> BorrowCast for [R] {
    type AsConst = [R::AsConst];
}
unsafe impl<R: BorrowCastMut<AsMut: Copy>> BorrowCastMut for [R] {
    type AsMut = [R::AsMut];
}

impl<R: ReprFamily, const N: usize> ReprFamily for [R; N] {
    type Kind = R::Kind;
}
impl<T, const N: usize> SizeFamily for [T; N] {
    type Kind = crate::size::Sized;
}

impl<R: ExternC<CType: Sized>, const N: usize> ExternC for [R; N] {
    type CType = [R::CType; N];
}
impl<R: EncodeOwned<CType: Copy>, const N: usize> EncodeOwned for [R; N] {
    type Store = ArrayStore<R::Store, N>;

    fn soft_encode_owned<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        assert_arr_has_non_zero_len::<N>();

        let store = &mut store.0;

        let mut items = self.into_iter();
        let mut stores = store.iter_mut();

        core::array::from_fn(|_| {
            let item = items.next().unwrap();
            let store = stores.next().unwrap();

            item.soft_encode_owned(store)
        })
    }
}
impl<'d, R: DecodeOwned<'d, CType: Copy>, const N: usize> DecodeOwned<'d> for [R; N] {
    type Store = ArrayStore<R::Store, N>;

    unsafe fn soft_decode_owned<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        assert_arr_has_non_zero_len::<N>();

        let mut stores = store.0.iter_mut();
        let decoded =
            source.map(|item| unsafe { R::soft_decode_owned(item, stores.next().unwrap()) });

        if decoded.iter().any(Option::is_none) {
            return None;
        }

        Some(decoded.map(|item| unsafe { item.unwrap_unchecked() }))
    }
}

unsafe impl<R: RobustReprC, const N: usize> RobustReprC for [R; N] {}

unsafe impl<R: BorrowCast<AsConst: Copy> + Copy, const N: usize> BorrowCast for [R; N] {
    type AsConst = [R::AsConst; N];
}
unsafe impl<R: BorrowCastMut<AsMut: Copy> + Copy, const N: usize> BorrowCastMut for [R; N] {
    type AsMut = [R::AsMut; N];
}

unsafe impl<T: EmptyStore, const N: usize> EmptyStore for [T; N] {}
// TODO: It's not possbile to implement for specific len yet: https://github.com/mversic/co3/issues/13
//unsafe impl<T> EmptyStore for [T; 0] {}

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
            ReprFamily<Kind = ReprC<Robust>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = u8>,
            Decode<'static>,
            Encode,
            RobustReprC,
        );
        assert_impl_all!(&u8:
            ReprFamily<Kind = ReprC<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut u8:
            ReprFamily<Kind = ReprC<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<u8>:
            ReprFamily<Kind = ReprC<NonRobust>>,
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
            ReprFamily<Kind = ReprC<Robust>>,
            NicheFamily<Kind = WithoutNiche>,
            Decode<'static>,
            Encode,
            RobustReprC,
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
