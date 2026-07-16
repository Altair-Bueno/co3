#[cfg(feature = "alloc")]
use alloc_crate::{string::String, vec::Vec};
use core::{
    cell::{Cell, UnsafeCell},
    ffi::c_void,
    marker::PhantomData,
    mem::ManuallyDrop,
    num::NonZero,
    ops::Add,
    ptr::NonNull,
};

use crate::{
    Decode, Encode, ExternC, NonRobust, RobustReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, ToOwned},
    ir::{ReprC, ReprFamily, Robust},
    niche::{Niche, NicheFamily, StableNiche, WithStableNiche, WithoutNiche},
    size::{MetaSized, SizeFamily, SliceLike},
    stored::{DecodeOwned, EmptyStore, EncodeOwned},
    transmute::CheckedTransmute,
};
#[cfg(feature = "alloc")]
use crate::{boxed::CBoxedSlice, ir::ReprRust, niche::WithCustomNiche};

macro_rules! non_zero_derive {
    ($($primitive:ty),+ $(,)?) => {$(
        impl ReprFamily for NonZero<$primitive> {
            type Kind = ReprC<NonRobust>;
        }
        unsafe impl SizeFamily for NonZero<$primitive> {
            type Kind = crate::size::Sized<crate::size::NonZst>;
        }
        impl NicheFamily for NonZero<$primitive> {
            type Kind = WithStableNiche;
        }

        unsafe impl Borrow for NonZero<$primitive> {
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
        impl<'itm> ToOwned<'itm> for NonZero<$primitive> {
            #[inline(always)]
            fn to_owned(source: Self::Borrowed<'itm>) -> Self {
                source
            }
        }

        impl ExternC for NonZero<$primitive> {
            type CType = $primitive;
        }
        unsafe impl EncodeOwned for NonZero<$primitive> {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm
            {
                self.get()
            }
        }
        unsafe impl<'d> DecodeOwned<'d> for NonZero<$primitive> {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Self::new(source)
            }
        }

        impl Encode for NonZero<$primitive> {}
        impl Decode<'_> for NonZero<$primitive> {}

        unsafe impl CheckedTransmute for NonZero<$primitive> {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                *target != 0
            }
        }

        impl Niche for NonZero<$primitive> {
            const NICHE_VALUE: Self::CType = 0;
        }
        unsafe impl StableNiche for NonZero<$primitive> {}

        )+
    }
}

non_zero_derive! {
    u8, i8, u16, i16, u32, i32, u64, i64, u128, i128,
}

impl ReprFamily for c_void {
    type Kind = ReprC<Robust>;
}
unsafe impl SizeFamily for c_void {
    // TODO: Shouldn't it be ExternTypeLike?
    // I know that `mem::size_of` returns 1
    type Kind = crate::size::Sized<crate::size::NonZst>;
}
impl NicheFamily for c_void {
    type Kind = WithoutNiche;
}

impl ExternC for c_void {
    type CType = Self;
}
unsafe impl RobustReprC for c_void {}
unsafe impl BorrowCast for c_void {
    type AsConst = Self;
}
unsafe impl BorrowCastMut for c_void {
    type AsMut = Self;
}

impl ReprFamily for () {
    type Kind = ReprC<Robust>;
}
unsafe impl SizeFamily for () {
    type Kind = crate::size::Sized<crate::size::Zst>;
}
impl NicheFamily for () {
    type Kind = WithoutNiche;
}

unsafe impl Borrow for () {
    type Borrowed<'itm>
        = Self
    where
        Self: 'itm;

    type Owner = ();

    #[inline(always)]
    fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm> {}
}
impl<'itm> ToOwned<'itm> for () {
    #[inline(always)]
    fn to_owned(_: ()) -> Self {}
}

impl ExternC for () {
    type CType = Self;
}
unsafe impl EncodeOwned for () {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self
    }
}
unsafe impl<'d> DecodeOwned<'d> for () {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}

impl Encode for () {}
impl Decode<'_> for () {}

unsafe impl RobustReprC for () {}
unsafe impl BorrowCast for () {
    type AsConst = Self;
}
unsafe impl BorrowCastMut for () {
    type AsMut = Self;
}

unsafe impl EmptyStore for () {}

impl<T: ?Sized> ReprFamily for PhantomData<T> {
    type Kind = ReprC<Robust>;
}
unsafe impl<T: ?Sized> SizeFamily for PhantomData<T> {
    type Kind = crate::size::Sized<crate::size::Zst>;
}
impl<T: ?Sized> NicheFamily for PhantomData<T> {
    type Kind = WithoutNiche;
}

unsafe impl<T: ?Sized> Borrow for PhantomData<T> {
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
impl<'itm, T: ?Sized + 'itm> ToOwned<'itm> for PhantomData<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

impl<T: ?Sized> ExternC for PhantomData<T> {
    type CType = Self;
}
unsafe impl<T: ?Sized> EncodeOwned for PhantomData<T> {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self
    }
}
unsafe impl<'d, T: ?Sized> DecodeOwned<'d> for PhantomData<T> {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}

impl<T: ?Sized> Encode for PhantomData<T> {}
impl<T: ?Sized> Decode<'_> for PhantomData<T> {}

unsafe impl<T: ?Sized> RobustReprC for PhantomData<T> {}
unsafe impl<T: ?Sized> BorrowCast for PhantomData<T> {
    type AsConst = Self;
}
unsafe impl<T: ?Sized> BorrowCastMut for PhantomData<T> {
    type AsMut = Self;
}

impl<T: ReprFamily<Kind: Add<ReprC<NonRobust>>> + ?Sized> ReprFamily for NonNull<T> {
    type Kind = <T::Kind as Add<ReprC<NonRobust>>>::Output;
}
unsafe impl<T: ?Sized> SizeFamily for NonNull<T> {
    type Kind = crate::size::Sized<crate::size::NonZst>;
}
impl<T: ?Sized> NicheFamily for NonNull<T> {
    type Kind = WithStableNiche;
}

unsafe impl<T: ?Sized> Borrow for NonNull<T> {
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
impl<'itm, T: ?Sized + 'itm> ToOwned<'itm> for NonNull<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

impl<T: RobustReprC + ?Sized> ExternC for NonNull<T> {
    // TODO: afaik the pointer is not necessarily mutable just non-null
    type CType = <*mut T as ExternC>::CType;
}
unsafe impl<T: RobustReprC + ?Sized> EncodeOwned for NonNull<T> {
    type Store = <*mut T as EncodeOwned>::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self.as_ptr().soft_encode(store)
    }
}
unsafe impl<'d, T: RobustReprC + ?Sized> DecodeOwned<'d> for NonNull<T> {
    type Store = <*mut T as DecodeOwned<'d>>::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        let ptr = unsafe { DecodeOwned::soft_decode(source, store)? };
        NonNull::new(ptr)
    }
}

impl<T: RobustReprC + ?Sized> Encode for NonNull<T> {}
impl<T: RobustReprC + ?Sized> Decode<'_> for NonNull<T> {}

unsafe impl<T: RobustReprC + ?Sized> CheckedTransmute for NonNull<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        !target.is_null()
    }
}

impl ReprFamily for str {
    type Kind = ReprC<NonRobust>;
}
unsafe impl SizeFamily for str {
    type Kind = MetaSized<SliceLike>;
}

impl ExternC for str {
    type CType = [u8];
}

#[cfg(feature = "alloc")]
impl ReprFamily for String {
    type Kind = ReprRust;
}
#[cfg(feature = "alloc")]
unsafe impl SizeFamily for String {
    type Kind = crate::size::Sized<crate::size::NonZst>;
}
#[cfg(feature = "alloc")]
impl NicheFamily for String {
    type Kind = WithCustomNiche;
}

#[cfg(feature = "alloc")]
unsafe impl Borrow for String {
    type Borrowed<'itm>
        = &'itm str
    where
        Self: 'itm;

    type Owner = Self;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        *owner = self;
        owner
    }
}
#[cfg(feature = "alloc")]
impl<'itm> ToOwned<'itm> for String {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source.into()
    }
}

#[cfg(feature = "alloc")]
impl ExternC for String {
    type CType = <Vec<u8> as ExternC>::CType;
}
#[cfg(feature = "alloc")]
unsafe impl EncodeOwned for String {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        crate::stored::encode_owned(self.into_bytes())
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d> DecodeOwned<'d> for String {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        let bytes = unsafe { crate::stored::decode_owned(source)? };
        String::from_utf8(bytes).ok()
    }
}

#[cfg(feature = "alloc")]
impl Encode for String {}
#[cfg(feature = "alloc")]
impl Decode<'_> for String {}

#[cfg(feature = "alloc")]
impl Niche for String {
    const NICHE_VALUE: Self::CType = CBoxedSlice::NICHE_VALUE;
}

impl<T: ReprFamily + ?Sized> ReprFamily for UnsafeCell<T> {
    type Kind = T::Kind;
}
unsafe impl<T: SizeFamily + ?Sized> SizeFamily for UnsafeCell<T> {
    type Kind = T::Kind;
}
impl<T: ?Sized> NicheFamily for UnsafeCell<T> {
    type Kind = WithoutNiche;
}

unsafe impl<T: Borrow> Borrow for UnsafeCell<T> {
    type Borrowed<'itm>
        = T::Borrowed<'itm>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self.into_inner().borrow(owner)
    }
}
impl<'itm, T: ToOwned<'itm>> ToOwned<'itm> for UnsafeCell<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        UnsafeCell::new(T::to_owned(source))
    }
}

impl<R: ExternC + ?Sized> ExternC for UnsafeCell<R> {
    type CType = R::CType;
}
unsafe impl<R: EncodeOwned<CType: Copy>> EncodeOwned for UnsafeCell<R> {
    type Store = R::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self.into_inner().soft_encode(store)
    }
}
unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>> DecodeOwned<'d> for UnsafeCell<R> {
    type Store = R::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { R::soft_decode(source, store) }.map(Self::new)
    }
}

impl<R: Encode<CType: Copy>> Encode for UnsafeCell<R> {}
impl<'d, R: Decode<'d, CType: Copy>> Decode<'d> for UnsafeCell<R> {}

unsafe impl<T: CheckedTransmute + ?Sized> CheckedTransmute for UnsafeCell<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(target) }
    }
}

unsafe impl<T: EmptyStore> EmptyStore for UnsafeCell<T> {}

impl<T: ReprFamily + ?Sized> ReprFamily for Cell<T> {
    type Kind = T::Kind;
}
unsafe impl<T: SizeFamily + ?Sized> SizeFamily for Cell<T> {
    type Kind = T::Kind;
}
impl<T: ?Sized> NicheFamily for Cell<T> {
    type Kind = WithoutNiche;
}

unsafe impl<T: Borrow> Borrow for Cell<T> {
    type Borrowed<'itm>
        = T::Borrowed<'itm>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        T::borrow(Cell::into_inner(self), owner)
    }
}
impl<'itm, T: ToOwned<'itm>> ToOwned<'itm> for Cell<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        Cell::new(T::to_owned(source))
    }
}

impl<R: ExternC + ?Sized> ExternC for Cell<R> {
    type CType = R::CType;
}
unsafe impl<R: EncodeOwned<CType: Copy>> EncodeOwned for Cell<R> {
    type Store = R::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self.into_inner().soft_encode(store)
    }
}
unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>> DecodeOwned<'d> for Cell<R> {
    type Store = R::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { R::soft_decode(source, store) }.map(Self::new)
    }
}

impl<R: Encode<CType: Copy>> Encode for Cell<R> {}
impl<'d, R: Decode<'d, CType: Copy>> Decode<'d> for Cell<R> {}

unsafe impl<T: CheckedTransmute + ?Sized> CheckedTransmute for Cell<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(target) }
    }
}

unsafe impl<T: EmptyStore> EmptyStore for Cell<T> {}

impl<T: ReprFamily + ?Sized> ReprFamily for ManuallyDrop<T> {
    type Kind = T::Kind;
}
unsafe impl<T: SizeFamily + ?Sized> SizeFamily for ManuallyDrop<T> {
    type Kind = T::Kind;
}
impl<T: NicheFamily + ?Sized> NicheFamily for ManuallyDrop<T> {
    type Kind = T::Kind;
}

unsafe impl<T: Borrow> Borrow for ManuallyDrop<T> {
    type Borrowed<'itm>
        = T::Borrowed<'itm>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        ManuallyDrop::into_inner(self).borrow(owner)
    }
}
impl<'itm, T: ToOwned<'itm>> ToOwned<'itm> for ManuallyDrop<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        ManuallyDrop::new(T::to_owned(source))
    }
}

impl<T: ExternC + ?Sized> ExternC for ManuallyDrop<T> {
    type CType = T::CType;
}
// FIXME: I think t's not ok to get owned type and encode it
// ManuallyDrop::into_inner(self).soft_encode(store)
//unsafe impl<R: EncodeOwned<CType: Copy>> EncodeOwned for ManuallyDrop<R> {
//    type Store = R::Store;
//
//    #[inline(always)]
//    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
//    where
//        Self: 'itm,
//    {
//        unimplemented!()
//    }
//}
//unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>> DecodeOwned<'d> for ManuallyDrop<R> {
//    type Store = R::Store;
//
//    #[inline(always)]
//    unsafe fn soft_decode<'itm: 'd>(
//        source: Self::CType,
//        store: &'itm mut Self::Store,
//    ) -> Option<Self> {
//        unimplemented!()
//    }
//}

//impl<R: Encode<CType: Copy>> Encode for ManuallyDrop<R> {}
//impl<'d, R: Decode<'d, CType: Copy>> Decode<'d> for ManuallyDrop<R> {}

unsafe impl<T: CheckedTransmute + ?Sized> CheckedTransmute for ManuallyDrop<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(target) }
    }
}

impl<T: Niche> Niche for ManuallyDrop<T> {
    const NICHE_VALUE: Self::CType = T::NICHE_VALUE;
}
unsafe impl<T: StableNiche> StableNiche for ManuallyDrop<T> {}

unsafe impl<T: EmptyStore> EmptyStore for ManuallyDrop<T> {}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc_crate::{boxed::Box, vec};

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::{CBox, CBoxedSlice};
    use crate::{
        Decode, Encode, ReprRust,
        niche::WithCustomNiche,
        option::ReprCOption,
        size::{NonZst, Sized as Co3Sized},
        slice::{CSlice, CSliceMut},
    };

    // TODO: Enable
    //#[test]
    //fn manually_drop_inner_without_drop() {
    //    assert_impl_all!(ManuallyDrop<u8>:
    //        ReprFamily<Kind = ReprC<Robust>>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithoutNiche>,
    //        ExternC<CType = u8>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&ManuallyDrop<u8>:
    //        ReprFamily<Kind = ReprC<NonRobust>>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //        StableNiche<CType = *const u8>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&mut ManuallyDrop<u8>:
    //        ReprFamily<Kind = ReprC<NonRobust>>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //        StableNiche<CType = *mut u8>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    #[cfg(feature = "alloc")]
    //    assert_impl_all!(Box<ManuallyDrop<u8>>:
    //        ReprFamily<Kind = ReprC<NonRobust>>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //        StableNiche<CType = CBox<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&[ManuallyDrop<u8>]:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //        Niche<CType = CSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&mut [ManuallyDrop<u8>]:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //        Niche<CType = CSliceMut<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    #[cfg(feature = "alloc")]
    //    assert_impl_all!(Box<[ManuallyDrop<u8>]>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //        Niche<CType = CBoxedSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    #[cfg(feature = "alloc")]
    //    assert_impl_all!(Vec<ManuallyDrop<u8>>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //        Niche<CType = CBoxedSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!([ManuallyDrop<u8>; 2]:
    //        ReprFamily<Kind = ReprC<Robust>>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithoutNiche>,
    //        ExternC<CType = [u8; 2]>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Option<ManuallyDrop<u8>>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //        Niche<CType = ReprCOption<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );

    //    assert_not_impl_any!(ManuallyDrop<u8>: RobustReprC);
    //}

    //#[cfg(feature = "alloc")]
    //#[test]
    //fn manually_drop_inner_with_drop() {
    //    assert_impl_all!(ManuallyDrop<String>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //        Niche<CType = CBoxedSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&ManuallyDrop<String>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //        StableNiche<CType = *const CBoxedSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&mut ManuallyDrop<String>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //        StableNiche<CType = *mut CBoxedSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Box<ManuallyDrop<String>>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //        StableNiche<CType = CBox<CBoxedSlice<u8>>>,
    //        DecodeOwned<'static>,
    //        EncodeOwned,
    //    );
    //    assert_impl_all!(&[ManuallyDrop<String>]:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //        Niche<CType = CSlice<CBoxedSlice<u8>>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&mut [ManuallyDrop<String>]:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //        Niche<CType = CSliceMut<CBoxedSlice<u8>>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Box<[ManuallyDrop<String>]>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //        Niche<CType = CBoxedSlice<CBoxedSlice<u8>>>,
    //        DecodeOwned<'static>,
    //        EncodeOwned,
    //    );
    //    assert_impl_all!(Vec<ManuallyDrop<String>>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //        Niche<CType = CBoxedSlice<CBoxedSlice<u8>>>,
    //        DecodeOwned<'static>,
    //        EncodeOwned,
    //    );
    //    assert_impl_all!([ManuallyDrop<String>; 2]:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //        Niche<CType = [CBoxedSlice<u8>; 2]>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Option<ManuallyDrop<String>>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        // FIXME:
    //        //NicheFamily<Kind = WithCustomNiche>,
    //        //Niche<CType = CBoxedSlice<u8>>,
    //        ExternC<CType = CBoxedSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_not_impl_any!(ManuallyDrop<String>: RobustReprC);

    //    #[cfg(feature = "alloc")]
    //    assert_not_impl_any!(Box<[ManuallyDrop<String>]>: Encode, Decode<'static>);
    //    #[cfg(feature = "alloc")]
    //    assert_not_impl_any!(Vec<ManuallyDrop<String>>: Encode, Decode<'static>);
    //}

    #[test]
    fn str_is_supported() {
        assert_impl_all!(str:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = MetaSized<SliceLike>>,
        );

        assert_impl_all!(&str:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut str:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<str>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn str_decode_rejects_invalid_utf8() {
        assert_impl_all!(str: SizeFamily<Kind = MetaSized<SliceLike>>);
        assert_impl_all!(&str: SizeFamily<Kind = Co3Sized<NonZst>>);
        assert_impl_all!(&mut str: SizeFamily<Kind = Co3Sized<NonZst>>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<str>: SizeFamily<Kind = Co3Sized<NonZst>>);

        let invalid = [0xff];

        let source = CSlice::from_raw_parts(invalid.as_ptr(), invalid.len());
        let decoded = unsafe { crate::decode::<&str>(source) };
        assert!(decoded.is_none());

        let mut invalid = [0xff];
        let source = CSliceMut::from_raw_parts_mut(invalid.as_mut_ptr(), invalid.len());
        let decoded = unsafe { crate::decode::<&mut str>(source) };
        assert!(decoded.is_none());

        #[cfg(feature = "alloc")]
        {
            let source = CBoxedSlice::from_boxed_slice(vec![0xff].into_boxed_slice());
            let decoded = unsafe { crate::decode::<Box<str>>(source) };
            assert!(decoded.is_none());
        }
    }

    #[test]
    fn robust_unsafe_cell() {
        assert_impl_all!(UnsafeCell<u8>:
            ReprFamily<Kind = ReprC<Robust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&UnsafeCell<u8>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            // FIXME: UnsafeCell refs are mut
            // StableNiche<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut UnsafeCell<u8>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<u8>>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[UnsafeCell<u8>]:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [UnsafeCell<u8>]:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<u8>]>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<u8>>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([UnsafeCell<u8>; 2]:
            ReprFamily<Kind = ReprC<Robust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = [u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<UnsafeCell<u8>>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = ReprCOption<u8>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(UnsafeCell<u8>: RobustReprC);
    }

    #[test]
    fn non_robust_unsafe_cell() {
        assert_impl_all!(UnsafeCell<NonZero<u8>>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&UnsafeCell<NonZero<u8>>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            // FIXME: UnsafeCell refs are mut
            // StableNiche<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut UnsafeCell<NonZero<u8>>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<NonZero<u8>>>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[UnsafeCell<NonZero<u8>>]:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [UnsafeCell<NonZero<u8>>]:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<NonZero<u8>>]>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<NonZero<u8>>>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([UnsafeCell<NonZero<u8>>; 2]:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = [u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<UnsafeCell<NonZero<u8>>>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = ReprCOption<u8>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(UnsafeCell<NonZero<u8>>: RobustReprC);
    }
}
