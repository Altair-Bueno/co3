#[cfg(feature = "alloc")]
use alloc_crate::{string::String, vec::Vec};
use core::{
    cell::{Cell, UnsafeCell},
    ffi::c_void,
    mem::ManuallyDrop,
    ops::Add,
    ptr::NonNull,
};

use crate::{
    ExternC, NonRobust, ReprC,
    borrow::{Borrow, BorrowCast, ToOwned},
    handle::Erase,
    ir::{ReprFamily, Robust, Transmuted},
    niche::{Niche, NicheFamily, StableNiche, WithCustomNiche, WithStableNiche, WithoutNiche},
    reprC,
    size::{MetaSized, SizeFamily, SliceLike},
    stored::{SoftDecodeOwned, SoftEncodeOwned},
    transmute::CheckedTransmute,
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::CBoxedSlice,
    ir::ReprRust,
    stored::{DecodeOwned, EncodeOwned},
};

// FIXME: Replace with NonZero<T>
macro_rules! non_zero_derive {
    ($($ty:ty => $target:ty),+ $(,)?) => {$(
        reprC! {
            // TODO: Consider expanding this impl by hand. the macro call
            // adds too many unnecessary bounds
            unsafe impl NoDropSizedTransparent for $ty {
                const NICHE_VALUE: $target = 0;

                type Target = $target;
                fn is_valid(target: $target) -> bool {
                    *target != 0
                }
            }
        }

        impl SoftEncodeOwned for $ty {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm
            {
                self.get()
            }
        }

        impl<'d> SoftDecodeOwned<'d> for $ty {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Self::new(source)
            }
        }

        impl Borrow for $ty {
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

        impl<'itm> ToOwned<'itm> for $ty {
            #[inline(always)]
            fn to_owned(source: Self::Borrowed<'itm>) -> Self {
                source
            }
        }

        unsafe impl StableNiche for $ty {})+
    }
}

non_zero_derive! {
    core::num::NonZeroU8 => u8,
    core::num::NonZeroI8 => i8,
    core::num::NonZeroU16 => u16,
    core::num::NonZeroI16 => i16,
    core::num::NonZeroU32 => u32,
    core::num::NonZeroI32 => i32,
    core::num::NonZeroU64 => u64,
    core::num::NonZeroI64 => i64,
    core::num::NonZeroU128 => u128,
    core::num::NonZeroI128 => i128,
}

impl ReprFamily for c_void {
    type Kind = Transmuted<Robust>;
}
impl SizeFamily for c_void {
    // TODO: Shouldn't it be ExternTypeLike?
    // I know that `mem::size_of` returns 1
    type Kind = crate::size::Sized;
}
impl NicheFamily for c_void {
    type Kind = WithoutNiche;
}

// TODO: To support ZST types properly we should introduce better SizeFamily disambiguation
// then we can implement for any ZST type including PhantomData
impl ReprFamily for () {
    type Kind = Transmuted<Robust>;
}
impl SizeFamily for () {
    type Kind = crate::size::Sized;
}
impl NicheFamily for () {
    type Kind = WithoutNiche;
}

unsafe impl ReprC for () {}
unsafe impl BorrowCast for () {
    type AsConst = Self;
    type AsMut = Self;
}

impl ExternC for () {
    type CType = Self;
}
impl SoftEncodeOwned for () {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self
    }
}
impl<'d> SoftDecodeOwned<'d> for () {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}

impl<T: ReprFamily<Kind: Add<Transmuted<NonRobust>>> + ?Sized> ReprFamily for NonNull<T> {
    type Kind = <T::Kind as Add<Transmuted<NonRobust>>>::Output;
}
impl<T: ?Sized> SizeFamily for NonNull<T> {
    type Kind = crate::size::Sized;
}
impl<T: ?Sized> NicheFamily for NonNull<T> {
    type Kind = WithStableNiche;
}

unsafe impl<T: ReprC + ?Sized> CheckedTransmute for NonNull<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        !target.is_null()
    }
}

impl<T: ReprC + ?Sized> ExternC for NonNull<T> {
    // TODO: afaik the pointer is not necessarily mutable just non-null
    type CType = <*mut T as ExternC>::CType;
}
impl<T: ReprC + ?Sized> SoftEncodeOwned for NonNull<T> {
    type Store = <*mut T as SoftEncodeOwned>::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self.as_ptr().soft_encode(store)
    }
}
impl<'d, T: ReprC + ?Sized> SoftDecodeOwned<'d> for NonNull<T> {
    type Store = <*mut T as SoftDecodeOwned<'d>>::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        let ptr = unsafe { SoftDecodeOwned::soft_decode(source, store)? };
        NonNull::new(ptr)
    }
}

impl<T: ?Sized> Borrow for NonNull<T> {
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

unsafe impl<T: Erase + ?Sized> Erase for NonNull<T> {
    type Erased = NonNull<T::Erased>;
}
impl ReprFamily for str {
    type Kind = Transmuted<NonRobust>;
}
impl SizeFamily for str {
    type Kind = MetaSized<SliceLike>;
}

impl ExternC for str {
    type CType = <[u8] as ExternC>::CType;
}

#[cfg(feature = "alloc")]
impl ReprFamily for String {
    type Kind = ReprRust;
}
#[cfg(feature = "alloc")]
impl SizeFamily for String {
    type Kind = crate::size::Sized;
}
#[cfg(feature = "alloc")]
impl NicheFamily for String {
    type Kind = WithCustomNiche;
}

#[cfg(feature = "alloc")]
impl Niche for String {
    const NICHE_VALUE: Self::CType = CBoxedSlice::none();
}

#[cfg(feature = "alloc")]
impl ExternC for String {
    type CType = <Vec<u8> as ExternC>::CType;
}
#[cfg(feature = "alloc")]
impl SoftEncodeOwned for String {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self.into_bytes().encode()
    }
}
#[cfg(feature = "alloc")]
impl<'d> SoftDecodeOwned<'d> for String {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        let bytes = unsafe { DecodeOwned::decode(source)? };
        String::from_utf8(bytes).ok()
    }
}

impl<T: ReprFamily + ?Sized> ReprFamily for UnsafeCell<T> {
    type Kind = T::Kind;
}
impl<T: SizeFamily + ?Sized> SizeFamily for UnsafeCell<T> {
    type Kind = T::Kind;
}
impl<T> NicheFamily for UnsafeCell<T> {
    type Kind = WithoutNiche;
}

unsafe impl<T: CheckedTransmute + ?Sized> CheckedTransmute for UnsafeCell<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(target) }
    }
}

impl<R: ExternC + ?Sized> ExternC for UnsafeCell<R> {
    type CType = R::CType;
}
impl<R: SoftEncodeOwned> SoftEncodeOwned for UnsafeCell<R> {
    type Store = R::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self.into_inner().soft_encode(store)
    }
}
impl<'d, R: SoftDecodeOwned<'d>> SoftDecodeOwned<'d> for UnsafeCell<R> {
    type Store = R::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { R::soft_decode(source, store) }.map(Self::new)
    }
}

impl<T: Borrow> Borrow for UnsafeCell<T> {
    type Borrowed<'itm>
        = T::Borrowed<'itm>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self.into_inner().borrow(store)
    }
}
impl<'itm, T: ToOwned<'itm>> ToOwned<'itm> for UnsafeCell<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        UnsafeCell::new(T::to_owned(source))
    }
}

impl<T: ReprFamily + ?Sized> ReprFamily for Cell<T> {
    type Kind = T::Kind;
}
impl<T: SizeFamily + ?Sized> SizeFamily for Cell<T> {
    type Kind = T::Kind;
}
impl<T> NicheFamily for Cell<T> {
    type Kind = WithoutNiche;
}

unsafe impl<T: CheckedTransmute + ?Sized> CheckedTransmute for Cell<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(target) }
    }
}

impl<R: ExternC + ?Sized> ExternC for Cell<R> {
    type CType = R::CType;
}
impl<R: SoftEncodeOwned> SoftEncodeOwned for Cell<R> {
    type Store = R::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self.into_inner().soft_encode(store)
    }
}
impl<'d, R: SoftDecodeOwned<'d>> SoftDecodeOwned<'d> for Cell<R> {
    type Store = R::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { R::soft_decode(source, store) }.map(Self::new)
    }
}

impl<T: Borrow> Borrow for Cell<T> {
    type Borrowed<'itm>
        = T::Borrowed<'itm>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        T::borrow(Cell::into_inner(self), store)
    }
}
impl<'itm, T: ToOwned<'itm>> ToOwned<'itm> for Cell<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        Cell::new(T::to_owned(source))
    }
}

impl<T: ReprFamily + ?Sized> ReprFamily for ManuallyDrop<T> {
    type Kind = T::Kind;
}
impl<T: SizeFamily + ?Sized> SizeFamily for ManuallyDrop<T> {
    type Kind = T::Kind;
}
impl<T: NicheFamily> NicheFamily for ManuallyDrop<T> {
    type Kind = T::Kind;
}

unsafe impl<T: CheckedTransmute> CheckedTransmute for ManuallyDrop<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(target) }
    }
}

impl<T: ExternC + ?Sized> ExternC for ManuallyDrop<T> {
    type CType = T::CType;
}
impl<T: Niche> Niche for ManuallyDrop<T> {
    const NICHE_VALUE: Self::CType = T::NICHE_VALUE;
}
unsafe impl<T: StableNiche> StableNiche for ManuallyDrop<T> {}
impl<R: SoftEncodeOwned> SoftEncodeOwned for ManuallyDrop<R> {
    type Store = R::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        ManuallyDrop::into_inner(self).soft_encode(store)
    }
}
impl<'d, R: SoftDecodeOwned<'d>> SoftDecodeOwned<'d> for ManuallyDrop<R> {
    type Store = R::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { R::soft_decode(source, store) }.map(Self::new)
    }
}

impl<T> Borrow for ManuallyDrop<T> {
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
impl<'itm, T: 'itm> ToOwned<'itm> for ManuallyDrop<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc_crate::boxed::Box;
    use core::num::NonZeroU8;

    use static_assertions::assert_impl_all;

    use super::*;

    #[cfg(feature = "alloc")]
    use crate::boxed::CBoxedSlice;
    use crate::ir::ReprRust;
    use crate::{
        ExternC, SoftDecode, SoftEncode,
        option::COption,
        slice::{CSlice, CSliceMut},
    };

    #[test]
    fn str_is_supported() {
        assert_impl_all!(str:
            ReprFamily<Kind = Transmuted<NonRobust>>,
        );

        assert_impl_all!(&str:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            //Niche<CType = CSlice<u8>>,
            //SoftDecode<'static>,
            // FIXME:
            //SoftEncode,
        );
        // TODO: Add more assertions
    }

    #[test]
    fn unsafe_cell_is_without_niche() {
        assert_impl_all!(UnsafeCell<NonZeroU8>:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = u8>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&UnsafeCell<NonZeroU8>:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut UnsafeCell<NonZeroU8>:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            SoftDecode<'static>,
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<UnsafeCell<NonZeroU8>>:
        //    ReprFamily<Kind = Transmuted<NonRobust>>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut u8>,
        //    SoftDecode<'static>,
        //    SoftEncode,
        //);
        assert_impl_all!(&[UnsafeCell<NonZeroU8>]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut [UnsafeCell<NonZeroU8>]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            SoftDecode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<NonZeroU8>]>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //SoftDecode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<NonZeroU8>>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!([UnsafeCell<NonZeroU8>; 2]:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = [u8; 2]>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(Option<UnsafeCell<NonZeroU8>>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = COption<u8>>,
            SoftDecode<'static>,
            // FIXME:
            //SoftEncode,
        );

        // FIXME:
        //#[cfg(any(
        //    feature = "alloc",
        //))]
        //assert_impl_all!(&mut UnsafeCell<NonZeroU8>: SoftEncode);
        //#[cfg(any(
        //    feature = "alloc",
        //))]
        //assert_impl_all!(&mut [UnsafeCell<NonZeroU8>]: SoftEncode);
        //#[cfg(not(any(
        //    feature = "alloc",
        //)))]
        //assert_not_impl_any!(&mut UnsafeCell<NonZeroU8>: SoftEncode);
        //#[cfg(not(any(
        //    feature = "alloc",
        //)))]
        //assert_not_impl_any!(&mut [UnsafeCell<NonZeroU8>]: SoftEncode);
    }
}
