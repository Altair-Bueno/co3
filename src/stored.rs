#[cfg(feature = "alloc")]
use alloc_crate::{borrow::ToOwned as StdToOwned, boxed::Box, vec::Vec};
#[cfg(feature = "alloc")]
use core::ptr::NonNull;

use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::borrow::borrow_cast_mut;
use crate::{
    ExternC, assert_arr_has_non_zero_len,
    borrow::{Borrow, BorrowCast, ToOwned, borrow_cast},
    ir::{NonRobust, ReprFamily, ReprRust, Robust, Transmuted},
    niche::{Niche, NicheFamily, WithNiche, WithoutNiche},
    out_ptr::Zst,
    result::ReprCResult,
    size::{MetaSized, SizeFamily, SliceLike, Wide},
    slice::{CSlice, CSliceMut},
    transmute::CheckedTransmute,
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::{CBox, CBoxedSlice},
    size::Dst,
};

// TODO: Could the store just be synced on drop?
// FIXME: Encode types can never error during sync
pub trait Store: Sized {
    fn sync(self) -> Option<()>;
}

pub(crate) trait ReprRustOrTransmutedNonRobust {}
impl ReprRustOrTransmutedNonRobust for ReprRust {}
impl ReprRustOrTransmutedNonRobust for Transmuted<NonRobust> {}

disjoint_impls! {
    /// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
    pub trait SoftEncodeOwned: ExternC<CType: Sized> + Sized {
        /// Auxiliary storage used during conversion. If storage is not used, set the type to `()`.
        ///
        /// Use cases include:
        /// - Keeping the result of the conversion of references of [`Stored`] types
        /// - Storing mutable references that need to be updated in [`Store::sync`]
        ///
        /// Conceptually, serves a role similar to the "context" captured by a closure.
        type Store: Store + Default;

        /// Convert from [`Self`] into [`Self::CType`].
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm;
    }

    impl<R: ExternC + ?Sized> SoftEncodeOwned for &R
    where
        Self: ReprFamily<Kind = Transmuted<NonRobust>>,
        Self: CheckedTransmute<CType = *const <R as ExternC>::CType>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = core::ptr::from_ref(self);

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { core::mem::transmute_copy::<*const R, *const R::CType>(&ptr) }
        }
    }
    impl<R: ReprFamily<Kind = Transmuted<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> SoftEncodeOwned for &R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Data: CheckedTransmute, Metadata = usize>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let len = self.metadata();
            let ptr = self.as_ptr().cast();
            CSlice::from_raw_parts(ptr, len)
        }
    }
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = crate::size::Sized>> SoftEncodeOwned for &R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Clone + SoftEncodeOwned,
    {
        type Store = RefSizedEncodeStore<R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let owned = self.clone();
            let ctype = owned.soft_encode(&mut store.store);
            store.ctype.insert(ctype)
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind: Dst> + ?Sized> SoftEncodeOwned for &R
    where
        Self: ReprFamily<Kind = ReprRust> + ExternC<CType = <<<R as StdToOwned>::Owned as ExternC>::CType as BorrowCast>::AsConst>,
        R: StdToOwned<Owned: ExternC<CType: BorrowCast<AsConst: Sized> + Copy> + SoftEncodeOwned>,
    {
        type Store = RefDstEncodeStore<R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let owned = StdToOwned::to_owned(self);
            let ctype = owned.soft_encode(&mut store.store);
            borrow_cast(*store.ctype.insert(ctype))
        }
    }

    impl<R: ReprFamily<Kind = Transmuted<Robust>> + ExternC + ?Sized> SoftEncodeOwned for &mut R
    where
        Self: ReprFamily<Kind = Transmuted<NonRobust>>,
        Self: CheckedTransmute<CType = *mut <R as ExternC>::CType>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = core::ptr::from_mut(self);

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { core::mem::transmute_copy::<*mut R, *mut R::CType>(&ptr) }
        }
    }
    impl<R: ReprFamily<Kind = Transmuted<Robust>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized> SoftEncodeOwned for &mut R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Data: CheckedTransmute, Metadata = usize>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let len = self.metadata();
            let ptr = self.as_mut_ptr().cast();
            CSliceMut::from_raw_parts_mut(ptr, len)
        }
    }
    // TODO: The following 2 impls are duplicated. This is likely a deficiency in disjoint_impls!. Fix it there
    impl<'a, R: ReprFamily<Kind: ReprRustOrTransmutedNonRobust> + SizeFamily<Kind = crate::size::Sized>> SoftEncodeOwned
        for &'a mut R
    where
        Self: ReprFamily<Kind = Transmuted<NonRobust>> + ExternC<CType = *mut <R as ExternC>::CType>,
        R: Clone + SoftEncodeOwned + SoftDecodeOwned<'a>,
    {
        type Store = RefMutSizedEncodeStore<'a, R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);
            let owned = original.clone();
            let ctype = owned.soft_encode(&mut store.store);
            store.ctype.insert(ctype)
        }
    }
    impl<'a, R: ReprFamily<Kind: ReprRustOrTransmutedNonRobust> + SizeFamily<Kind = crate::size::Sized>> SoftEncodeOwned
        for &'a mut R
    where
        Self: ReprFamily<Kind = ReprRust> + ExternC<CType = *mut <R as ExternC>::CType>,
        R: Clone + SoftEncodeOwned + SoftDecodeOwned<'a>,
    {
        type Store = RefMutSizedEncodeStore<'a, R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);
            let owned = original.clone();
            let ctype = owned.soft_encode(&mut store.store);
            store.ctype.insert(ctype)
        }
    }
    // TODO: We should prevent Sized opaque types here because they can't be decoded
    #[cfg(feature = "alloc")]
    impl<'a, R: ReprFamily<Kind: ReprRustOrTransmutedNonRobust> + SizeFamily<Kind: Dst> + ?Sized> SoftEncodeOwned for &'a mut R
    where
        Self: ReprFamily<Kind = ReprRust> + ExternC<CType = <<<R as StdToOwned>::Owned as ExternC>::CType as BorrowCast>::AsMut>,
        R: StdToOwned<Owned: ExternC<CType: BorrowCast<AsMut: Sized> + Copy> + SoftEncodeOwned + SoftDecodeOwned<'a>>,
    {
        type Store = RefMutDstEncodeStore<'a, R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);
            let owned = StdToOwned::to_owned(&**original);
            let ctype = owned.soft_encode(&mut store.store);
            borrow_cast_mut(*store.ctype.insert(ctype))
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted<K>> + SoftEncodeOwned, K> SoftEncodeOwned for Box<R>
    where
        Self: ReprFamily<Kind = Transmuted<NonRobust>>,
        Self: CheckedTransmute<CType = CBox<<R as ExternC>::CType>>,
    {
        type Store = R::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            if core::mem::size_of::<R::Store>() == 0 {
                // TODO: Use Box::into_non_null when stable
                let ptr = Box::into_raw(self).cast();
                let non_null_ptr = unsafe { NonNull::new_unchecked(ptr) };
                return CBox::from_raw_parts(non_null_ptr);
            }

            CBox::from_box(Some(Box::new((*self).soft_encode(store))))
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Transmuted<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> SoftEncodeOwned for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Data: CheckedTransmute + SoftEncodeOwned, Metadata = usize>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type Store = Box<[<R::Data as SoftEncodeOwned>::Store]>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let len = self.metadata();
            let data = R::into_non_null(self);

            if core::mem::size_of::<<R::Data as SoftEncodeOwned>::Store>() == 0 {
                return CBoxedSlice::from_raw_parts(data.cast(), len);
            }

            // FIXME: Should it convert to Owned?
            unimplemented!()
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = crate::size::Sized>> SoftEncodeOwned
        for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: SoftEncodeOwned,
    {
        type Store = R::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CBox::from_box(Some(Box::new((*self).soft_encode(store))))
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind: Dst> + ?Sized> SoftEncodeOwned for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust> + ExternC<CType = <<R as StdToOwned>::Owned as ExternC>::CType>,
        R: StdToOwned<Owned: SoftEncodeOwned>,
        Self: Into<<R as StdToOwned>::Owned>,
    {
        type Store = <R::Owned as SoftEncodeOwned>::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.into().soft_encode(store)
        }
    }

    impl<R: NicheFamily<Kind = WithoutNiche> + SoftEncodeOwned<CType: Copy>> SoftEncodeOwned for Option<R> {
        type Store = R::Store;

        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.map(|v| v.soft_encode(store))
                .into()
        }
    }
    impl<R: NicheFamily<Kind: WithNiche> + SoftEncodeOwned<CType: Copy> + Niche> SoftEncodeOwned for Option<R> {
        type Store = R::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            if let Some(value) = self {
                return value.soft_encode(store);
            }

            R::NICHE_VALUE
        }
    }

    impl<
        R: NicheFamily<Kind = WithoutNiche> + SoftEncodeOwned<CType: Copy>,
        E: NicheFamily<Kind = WithoutNiche> + SoftEncodeOwned<CType: Copy>,
    > SoftEncodeOwned for Result<R, E>
    where
        Self: ReprFamily<Kind = ReprRust>,
    {
        // TODO: a union would save space, this issue is even more pronounced when deriving user-defined enums
        // Check other places, for instance Decoding
        type Store = (R::Store, E::Store);

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Ok(ok) => ReprCResult::Ok(ok.soft_encode(&mut store.0)),
                Err(err) => ReprCResult::Err(err.soft_encode(&mut store.1)),
            }
        }
    }
    // TODO: Implement for niche optimized Results
}

disjoint_impls! {
    pub trait SoftDecodeOwned<'d>: ExternC<CType: Sized> + Sized {
        type Store: Store + Default;

        /// Perform the conversion from [`Self::CType`] into [`Self`]
        ///
        /// # Safety
        ///
        /// - All conversions from a pointer must ensure pointer validity beforehand
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self>;
    }

    impl<'d, R: ExternC + ?Sized> SoftDecodeOwned<'d> for &'d R
    where
        Self: ReprFamily<Kind = Transmuted<NonRobust>>,
        Self: CheckedTransmute<CType = *const <R as ExternC>::CType>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { core::mem::transmute_copy::<*const R::CType, *const R>(&source).as_ref() }
        }
    }
    impl<'d, R: ReprFamily<Kind = Transmuted<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> SoftDecodeOwned<'d> for &'d R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: CheckedTransmute,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let source = unsafe { source.into_rust()? };

            if !source.iter().all(|item| unsafe { R::Data::is_valid(item) }) {
                return None;
            }

            let len = source.len();
            let ptr = source.as_ptr().cast();
            Some(unsafe { R::from_raw_parts(ptr, len) })
        }
    }
    impl<'d, R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = crate::size::Sized> + ToOwned<'d>> SoftDecodeOwned<'d>
        for &'d R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: ExternC<CType: BorrowCast> + Borrow<Borrowed<'d>: SoftDecodeOwned<'d>>,
        <R as Borrow>::Borrowed<'d>: ExternC<CType = <<R as ExternC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefSizedDecodeStore<R, <R::Borrowed<'d> as SoftDecodeOwned<'d>>::Store>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let source = borrow_cast(unsafe { source.read() });

            let value = unsafe {
                <R::Borrowed<'d> as SoftDecodeOwned>::soft_decode(source, &mut store.store)?
            };

            Some(store.value.insert(ToOwned::to_owned(value)))
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = MetaSized<SliceLike>> + StdToOwned + ?Sized>
        SoftDecodeOwned<'d> for &'d R
    where
        Self: ReprFamily<Kind = ReprRust> + ExternC<CType: Sized>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: SoftDecodeOwned<'d>,
    {
        type Store = RefDstDecodeStore<R, Box<[<R::Data as SoftDecodeOwned<'d>>::Store]>>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unimplemented!()
            //let source = unsafe { source.into_rust()? };

            //*store.store = core::iter::repeat_with(Default::default)
            //    .take(source.len())
            //    .collect();

            //let owned = source
            //    .iter()
            //    .cloned()
            //    .zip(store.store.iter_mut())
            //    .map(|(item, store)| unsafe { R::Data::decode(item.view(), store) })
            //    .collect::<Option<Vec<_>>>()?;

            //Some(core::borrow::Borrow::borrow(store.value.insert(owned)))
        }
    }

    impl<'d, R: ExternC + ?Sized> SoftDecodeOwned<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = Transmuted<NonRobust>>,
        Self: CheckedTransmute<CType = *mut <R as ExternC>::CType>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { core::mem::transmute_copy::<*mut R::CType, *mut R>(&source).as_mut() }
        }
    }
    impl<'d, R: ReprFamily<Kind = Transmuted<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> SoftDecodeOwned<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: CheckedTransmute,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let source = unsafe { source.into_rust()? };

            if !source.iter().all(|item| unsafe { R::Data::is_valid(item) }) {
                return None;
            }

            let len = source.len();
            let ptr = source.as_mut_ptr().cast();
            Some(unsafe { R::from_raw_parts_mut(ptr, len) })
        }
    }
    impl<'d, R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = crate::size::Sized> + ToOwned<'d>> SoftDecodeOwned<'d>
        for &'d mut R
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: ExternC<CType: BorrowCast> + Borrow<Borrowed<'d>: SoftDecodeOwned<'d>>,
        <R as Borrow>::Borrowed<'d>: ExternC<CType = <<R as ExternC>::CType as BorrowCast>::AsConst>,
    {
        type Store =
            RefMutSizedDecodeStore<R, <R::Borrowed<'d> as SoftDecodeOwned<'d>>::Store>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let source = borrow_cast(unsafe { source.read() });

            let value = unsafe {
                <R::Borrowed<'d> as SoftDecodeOwned>::soft_decode(source, &mut store.store)?
            };
            Some(store.value.insert(ToOwned::to_owned(value)))
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = MetaSized<SliceLike>> + StdToOwned + ?Sized>
        SoftDecodeOwned<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = ReprRust> + ExternC<CType: Sized>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: SoftDecodeOwned<'d>,
    {
        type Store = RefMutDstDecodeStore<R, Box<[<R::Data as SoftDecodeOwned<'d>>::Store]>>;

        unsafe fn soft_decode<'itm: 'd>(_: Self::CType, _: &'itm mut Self::Store) -> Option<Self> {
            unimplemented!()
            //let value = unsafe { R::Owned::decode(source, &mut store.store)? };

            //Some(core::borrow::BorrowMut::borrow_mut(
            //    store.value.insert(value),
            //))
        }
    }

    #[cfg(feature = "alloc")]
    impl<'d, R: ExternC<CType: Sized>> SoftDecodeOwned<'d> for Box<R>
    where
        Self: ReprFamily<Kind = Transmuted<NonRobust>>,
        Self: CheckedTransmute<CType = CBox<<R as ExternC>::CType>>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            let ptr = CBox::into_raw(source).cast();
            Some(unsafe { Box::from_raw(ptr) })
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = Transmuted<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> SoftDecodeOwned<'d> for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: CheckedTransmute,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let source = unsafe { source.into_rust()? };

            if !source.iter().all(|item| unsafe { R::Data::is_valid(item) }) {
                return None;
            }

            let len = source.len();
            let ptr = source.into_non_null().cast();
            Some(unsafe { R::from_non_null(ptr, len) })
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind = crate::size::Sized>> SoftDecodeOwned<'d>
        for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust>,
        R: SoftDecodeOwned<'d, CType: Sized>,
    {
        type Store = R::Store;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            let source = unsafe { source.read() };
            let value = unsafe { R::soft_decode(source, store)? };
            Some(Box::new(value))
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: ReprFamily<Kind = ReprRust> + SizeFamily<Kind: Dst> + StdToOwned + ?Sized>
        SoftDecodeOwned<'d> for Box<R>
    where
        Self: ReprFamily<Kind = ReprRust> + ExternC<CType = <<R as StdToOwned>::Owned as ExternC>::CType>,
        <R as StdToOwned>::Owned: SoftDecodeOwned<'d> + Into<Self>,
    {
        type Store = <R::Owned as SoftDecodeOwned<'d>>::Store;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { R::Owned::soft_decode(source, store) }.map(Into::into)
        }
    }

    impl<'d, R: NicheFamily<Kind = WithoutNiche> + SoftDecodeOwned<'d>> SoftDecodeOwned<'d>
        for Option<R>
    where
        <Self as ExternC>::CType: Copy,
    {
        type Store = R::Store;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            match source.try_into().ok()? {
                Some(source) => unsafe { R::soft_decode(source, store) }.map(Some),
                None => Some(None),
            }
        }
    }
    impl<'d, R: NicheFamily<Kind: WithNiche> + SoftDecodeOwned<'d> + Niche<CType: PartialEq>>
        SoftDecodeOwned<'d> for Option<R>
    where
        <Self as ExternC>::CType: Copy,
    {
        type Store = <R as SoftDecodeOwned<'d>>::Store;

        unsafe fn soft_decode<'itm: 'd>(source: R::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source == R::NICHE_VALUE {
                return Some(None);
            }

            unsafe { R::soft_decode(source, store) }.map(Some)
        }
    }

    impl<
        'd,
        R: NicheFamily<Kind = WithoutNiche> + SoftDecodeOwned<'d, CType: Copy>,
        E: NicheFamily<Kind = WithoutNiche> + SoftDecodeOwned<'d, CType: Copy>,
    >
        SoftDecodeOwned<'d> for Result<R, E>
    where
        Self: ReprFamily<Kind = ReprRust>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            let value = match source.try_into().ok()? {
                Ok(ok) => {
                    let ok_store = store.insert(Ok(Default::default()));
                    let ok_store = unsafe { ok_store.as_mut().unwrap_unchecked() };
                    Ok(unsafe { R::soft_decode(ok, ok_store)? })
                }
                Err(err) => {
                    let err_store = store.insert(Err(Default::default()));
                    let err_store = unsafe { err_store.as_mut().unwrap_err_unchecked() };
                    Err(unsafe { E::soft_decode(err, err_store)? })
                }
            };

            Some(value)
        }
    }
    // TODO: Implement for niche optimized Results
}

pub trait EncodeOwned: SoftEncodeOwned {
    fn encode<'itm>(self) -> Self::CType
    where
        Self: 'itm;
}

pub trait DecodeOwned<'d>: SoftDecodeOwned<'d> {
    /// Perform the conversion from [`Self::CType`] into [`Self`]
    ///
    /// # Safety
    ///
    /// - All conversions from a pointer must ensure pointer validity beforehand
    unsafe fn decode(source: Self::CType) -> Option<Self>;
}

// TODO: Verify this impl for correctness and Decode as well
impl<R> EncodeOwned for R
where
    R: SoftEncodeOwned,
    <R as SoftEncodeOwned>::Store: Zst,
{
    fn encode<'itm>(self) -> Self::CType
    where
        Self: 'itm,
    {
        let mut store = Default::default();
        self.soft_encode(&mut store)
    }
}

impl<'d, R: SoftDecodeOwned<'d, Store: Zst> + 'd> DecodeOwned<'d> for R {
    unsafe fn decode(source: Self::CType) -> Option<Self> {
        let mut store = R::Store::default();
        // SAFETY: `DecodeFromRef` is only blanket-implemented for zero-sized stores, so extending
        // the borrow of the local store does not extend the lifetime of any backing data.
        let store = unsafe { core::mem::transmute::<&mut R::Store, &'d mut R::Store>(&mut store) };
        unsafe { R::soft_decode(source, store) }
    }
}

#[cfg(feature = "alloc")]
impl<R: SoftEncodeOwned> SoftEncodeOwned for Vec<R> {
    type Store = Box<[R::Store]>;

    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        *store = (0..self.len()).map(|_| Default::default()).collect();

        let ctypes = self
            .into_iter()
            .zip(store)
            .map(|(item, store)| item.soft_encode(store))
            .collect::<Box<[_]>>();

        CBoxedSlice::from_boxed_slice(Some(ctypes))
    }
}
#[cfg(feature = "alloc")]
impl<'d, R: SoftDecodeOwned<'d>> SoftDecodeOwned<'d> for Vec<R> {
    type Store = Box<[R::Store]>;

    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        let source = unsafe { source.into_rust()? };

        *store = core::iter::repeat_with(Default::default)
            .take(source.len())
            .collect();

        source
            .into_vec()
            .into_iter()
            .zip(store)
            .map(|(item, store)| unsafe { R::soft_decode(item, store) })
            .collect()
    }
}

impl<R: SoftEncodeOwned<CType: Copy>, const N: usize> SoftEncodeOwned for [R; N] {
    type Store = ArrayStore<R::Store, N>;

    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
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

            item.soft_encode(store)
        })
    }
}
impl<'d, R: SoftDecodeOwned<'d, CType: Copy>, const N: usize> SoftDecodeOwned<'d> for [R; N] {
    type Store = ArrayStore<R::Store, N>;

    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        assert_arr_has_non_zero_len::<N>();

        let mut stores = store.0.iter_mut();
        let decoded = source.map(|item| unsafe { R::soft_decode(item, stores.next().unwrap()) });

        if decoded.iter().any(Option::is_none) {
            return None;
        }

        Some(decoded.map(|item| unsafe { item.unwrap_unchecked() }))
    }
}

impl Store for () {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

#[cfg(feature = "alloc")]
impl<D: Store> Store for Box<[D]> {
    fn sync(self) -> Option<()> {
        for store in self {
            store.sync()?;
        }

        Some(())
    }
}

pub struct RefSizedEncodeStore<R: SoftEncodeOwned> {
    pub(crate) ctype: Option<R::CType>,
    pub(crate) store: R::Store,
}

#[cfg(feature = "alloc")]
pub struct RefDstEncodeStore<R: StdToOwned<Owned: SoftEncodeOwned> + ?Sized> {
    pub(crate) ctype: Option<<R::Owned as ExternC>::CType>,
    pub(crate) store: <R::Owned as SoftEncodeOwned>::Store,
}

pub struct RefMutSizedEncodeStore<'d, R: SoftEncodeOwned> {
    pub(crate) ctype: Option<R::CType>,
    pub(crate) store: R::Store,
    pub(crate) original: Option<&'d mut R>,
}

#[cfg(feature = "alloc")]
pub struct RefMutDstEncodeStore<'d, R: StdToOwned<Owned: SoftEncodeOwned> + ?Sized> {
    pub(crate) ctype: Option<<R::Owned as ExternC>::CType>,
    pub(crate) store: <R::Owned as SoftEncodeOwned>::Store,
    pub(crate) original: Option<&'d mut R>,
}

pub struct RefSizedDecodeStore<R, S> {
    pub(crate) value: Option<R>,
    pub(crate) store: S,
}

#[cfg(feature = "alloc")]
pub struct RefDstDecodeStore<R: StdToOwned + ?Sized, S> {
    pub(crate) value: Option<R::Owned>,
    pub(crate) store: S,
}

pub struct RefMutSizedDecodeStore<R, S> {
    pub(crate) value: Option<R>,
    pub(crate) store: S,
}

#[cfg(feature = "alloc")]
pub struct RefMutDstDecodeStore<R: StdToOwned + ?Sized, S> {
    pub(crate) value: Option<R::Owned>,
    pub(crate) store: S,
}
/// This struct exists only because [arrays don't yet implement Default](https://github.com/rust-lang/rust/issues/61415)
pub struct ArrayStore<D, const N: usize>(pub(crate) [D; N]);

// TODO: derive Default if macro is improved
impl<R: SoftEncodeOwned> Default for RefSizedEncodeStore<R> {
    fn default() -> Self {
        Self {
            ctype: None,
            store: Default::default(),
        }
    }
}

impl<R: SoftEncodeOwned> Store for RefSizedEncodeStore<R> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned<Owned: SoftEncodeOwned> + ?Sized> Default for RefDstEncodeStore<R> {
    fn default() -> Self {
        Self {
            ctype: None,
            store: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned<Owned: SoftEncodeOwned> + ?Sized> Store for RefDstEncodeStore<R> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

impl<'d, R: SoftEncodeOwned> Default for RefMutSizedEncodeStore<'d, R> {
    fn default() -> Self {
        Self {
            ctype: Default::default(),
            store: Default::default(),
            original: Default::default(),
        }
    }
}

impl<'d, R: SoftEncodeOwned + SoftDecodeOwned<'d>> Store for RefMutSizedEncodeStore<'d, R> {
    fn sync(self) -> Option<()> {
        // FIXME:
        //const {
        //    assert!(co3::impls!(R: Decode<'static>), "Not yet implemented");
        //}

        unimplemented!()
        //self.substore.sync()?;
        //if let (Some(ctype), Some(original)) = (self.ctype, self.original) {
        //    let mut decode_store = Default::default();
        //    let store_ref = unsafe {
        //        core::mem::transmute::<
        //            &mut <R as SoftDecodeOwned<'d>>::Store,
        //            &'d mut <R as SoftDecodeOwned<'d>>::Store,
        //        >(&mut decode_store)
        //    };

        //    *original = unsafe { R::decode(ctype, store_ref)? };
        //    decode_store.sync()?;
        //}

        //Some(())
    }
}

#[cfg(feature = "alloc")]
impl<'d, R: StdToOwned<Owned: SoftEncodeOwned> + ?Sized> Default for RefMutDstEncodeStore<'d, R> {
    fn default() -> Self {
        Self {
            ctype: Default::default(),
            store: Default::default(),
            original: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<'d, 'b, R: StdToOwned<Owned: SoftEncodeOwned + SoftDecodeOwned<'b>> + ?Sized> Store
    for RefMutDstEncodeStore<'d, R>
{
    fn sync(self) -> Option<()> {
        // FIXME:
        //const {
        //    assert!(co3::impls!(R: Decode<'static>), "Not yet implemented");
        //}

        unimplemented!()
    }
}

impl<R, S: Default> Default for RefSizedDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

impl<R, S: Store> Store for RefSizedDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned + ?Sized, S: Default> Default for RefDstDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned + ?Sized, S: Store> Store for RefDstDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

impl<R, S: Default> Default for RefMutSizedDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

impl<R, S: Store> Store for RefMutSizedDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        // FIXME:
        //const {
        //    assert!(co3::impls!(R: Decode<'static>), "Not yet implemented");
        //}

        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned + ?Sized, S: Default> Default for RefMutDstDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned + ?Sized, S: Store> Store for RefMutDstDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        // FIXME:
        //const {
        //    assert!(co3::impls!(R: Decode<'static>), "Not yet implemented");
        //}

        self.store.sync()
    }
}

impl<D: Default, const N: usize> Default for ArrayStore<D, N> {
    fn default() -> Self {
        Self(core::array::from_fn(|_| D::default()))
    }
}

impl<D: Store, const N: usize> Store for ArrayStore<D, N> {
    fn sync(self) -> Option<()> {
        for store in self.0 {
            store.sync()?;
        }

        Some(())
    }
}

unsafe impl<D: Zst, const N: usize> Zst for ArrayStore<D, N> {}

impl<T: Store> Store for Option<T> {
    fn sync(self) -> Option<()> {
        match self {
            Some(store) => store.sync(),
            None => Some(()),
        }
    }
}

impl<T: Store, E: Store> Store for Result<T, E> {
    fn sync(self) -> Option<()> {
        match self {
            Ok(store) => store.sync(),
            Err(store) => store.sync(),
        }
    }
}
