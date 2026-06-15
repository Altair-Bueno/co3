//! Logic related to the conversion of primitives to and from FFI-compatible representation

use crate::{
    CFnArg, CFnReturn, Decode, ExternC, ReprC,
    borrow::{Borrow, BorrowCast, ToOwned},
    handle::Erase,
    ir::{NonRobust, ReprFamily, ReprRust, Transmuted},
    niche::{Niche, NicheFamily, WithCustomNiche, WithoutNiche},
    reprC,
    size::{MetaSized, SizeFamily, SliceLike},
    stored::{SoftDecodeOwned, SoftEncodeOwned},
    transmute::CheckedTransmute,
};

macro_rules! primitive_derive {
    ( $($primitive:ty),* $(,)? ) => { $(
        reprC! { unsafe impl SizedRobust for $primitive {} } )*
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

        unsafe impl<R: ReprC + ?Sized> CheckedTransmute for *$mutability R {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<R: ReprC + ?Sized> ReprC for *$mutability R {}
        unsafe impl<R: ReprC + ?Sized> CFnArg for *$mutability R {}

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

        unsafe impl<R: Erase + ?Sized> Erase for *$mutability R {
            type Erased = *$mutability R::Erased;
        }
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

        impl ExternC for $src {
            type CType = $dst;
        }
        impl Niche for $src {
            const NICHE_VALUE: Self::CType = $niche_val;
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

        unsafe impl Erase for $src {
            type Erased = $src;
        }
    };
}

primitive_derive! { usize, isize, u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, f32, f64 }

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
impl<R: ExternC<CType: Copy>, const N: usize> ExternC for [R; N] {
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
