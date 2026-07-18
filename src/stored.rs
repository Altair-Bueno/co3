#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned as StdToOwned, boxed::Box, vec::Vec};
#[cfg(feature = "alloc")]
use core::ptr::NonNull;

use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::borrow::borrow_cast_mut;
#[cfg(feature = "alloc")]
use crate::boxed::{CBox, CBoxedSlice};
use crate::{
    Dst, ExternC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, ToOwned, borrow_cast},
    niche::{Niche, WithNiche},
    result::ReprCResult,
    slice::{CSlice, CSliceMut},
    transmute::CheckedTransmute,
};
use co3_types::{
    niche::{NicheFamily, WithoutNiche},
    repr::{NonRobust, Stable, ReprFamily, Unstable, Robust},
    size::{MetaSized, NonZst, SizeFamily, SliceLike, Wide, Zst},
};

// TODO: Could the store just be synced on drop?
// FIXME: Encode types can never error during sync
pub trait Store: Sized {
    fn sync(self) -> Option<()>;
}

/// Marker for an empty conversion store.
///
/// # Safety
///
/// Type must not contain any conversion state.
#[doc(hidden)]
pub unsafe trait EmptyStore: Sized {}

unsafe impl<T: EmptyStore> EmptyStore for Box<T> {}
unsafe impl<T: EmptyStore> EmptyStore for Box<[T]> {}
unsafe impl<T: EmptyStore> EmptyStore for Vec<T> {}

pub(crate) trait UnstableOrNonRobust {}
impl UnstableOrNonRobust for Unstable {}
impl UnstableOrNonRobust for Stable<NonRobust> {}

disjoint_impls! {
    /// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
    ///
    /// # Safety
    ///
    /// If [`Self::Store`] implements [`EmptyStore`], [`EncodeOwned::soft_encode`] must not
    /// return references into the store. [`crate::soft_encode`] correctness relies on it.
    ///
    /// Prefer using [`crate::soft_encode`] whenever possible
    ///
    pub unsafe trait EncodeOwned: ExternC<CType: Sized> + Sized {
        /// Auxiliary storage used during conversion. If storage is not used, set the type to `()`.
        ///
        /// Use cases include:
        /// - Keeping the result of the conversion of references of [`Stored`] types
        /// - Storing mutable references that need to be updated in [`Store::sync`]
        ///
        /// Conceptually, serves a role similar to the "context" captured by a closure.
        type Store: Store + Default;

        /// Convert from [`Self`] into [`Self::CType`].
        ///
        /// Prefer using [`crate::soft_encode`] whenever possible
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm;
    }

    unsafe impl<R: ExternC + ?Sized> EncodeOwned for &R
    where
        Self: ReprFamily<Kind = Stable<NonRobust>>,
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
    unsafe impl<R: ReprFamily<Kind = Stable<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K>
        EncodeOwned for &R
    where
        Self: ReprFamily<Kind = Unstable>,
        R: Wide<Data: CheckedTransmute<CType: Sized>, Metadata = usize>,
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
    unsafe impl<R: ReprFamily<Kind = Unstable> + SizeFamily<Kind = co3_types::size::Sized<S>>, S> EncodeOwned for &R
    where
        Self: ReprFamily<Kind = Unstable>,
        R: Clone + EncodeOwned,
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
    unsafe impl<R: ReprFamily<Kind = Unstable> + SizeFamily<Kind: Dst> + ?Sized> EncodeOwned for &R
    where
        Self: ReprFamily<Kind = Unstable>,
        Self: ExternC<CType = <<<R as StdToOwned>::Owned as ExternC>::CType as BorrowCast>::AsConst>,
        R: StdToOwned<Owned: ExternC<CType: BorrowCast<AsConst: Copy> + Copy> + EncodeOwned>,
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

    // TODO: SizeFamily should not be required here. That is a bug in disjoint_impls! Should be fixed soon
    unsafe impl<R: SizeFamily + ReprFamily<Kind = Stable<Robust>> + ExternC + ?Sized> EncodeOwned for &mut R
    where
        Self: ReprFamily<Kind = Stable<NonRobust>>,
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
    unsafe impl<R: SizeFamily<Kind = MetaSized<SliceLike>> + ReprFamily<Kind = Stable<Robust>> + ?Sized>
        EncodeOwned for &mut R
    where
        Self: ReprFamily<Kind = Unstable>,
        R: Wide<Data: CheckedTransmute<CType: Sized>, Metadata = usize>,
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
    unsafe impl<'a, R: SizeFamily<Kind = co3_types::size::Sized<S>> + ReprFamily<Kind: UnstableOrNonRobust>, S>
        EncodeOwned for &'a mut R
    where
        Self: ReprFamily<Kind: UnstableOrNonRobust>,
        Self: ExternC<CType = *mut <R as ExternC>::CType>,
        R: Clone + EncodeOwned + DecodeOwned<'a>,
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
    unsafe impl<'a, R: SizeFamily<Kind: Dst> + ReprFamily<Kind: UnstableOrNonRobust> + ?Sized>
        EncodeOwned for &'a mut R
    where
        Self: ReprFamily<Kind = Unstable>,
        Self: ExternC<CType = <<<R as StdToOwned>::Owned as ExternC>::CType as BorrowCastMut>::AsMut>,
        R: StdToOwned<Owned: ExternC<CType: BorrowCastMut<AsMut: Copy> + Copy> + EncodeOwned + DecodeOwned<'a>,
        >,
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
    unsafe impl<R: ReprFamily<Kind = Stable<K>> + EncodeOwned, K> EncodeOwned for Box<R>
    where
        Self: ReprFamily<Kind = Stable<NonRobust>>,
        Self: CheckedTransmute<CType = CBox<<R as ExternC>::CType>>,
    {
        type Store = Box<R::Store>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            if impls::impls!(R::Store: EmptyStore) {
                // TODO: Use Box::into_non_null when stable
                let ptr = Box::into_raw(self).cast();
                let non_null_ptr = unsafe { NonNull::new_unchecked(ptr) };
                return CBox::from_raw_parts(non_null_ptr);
            }

            CBox::from_box(Box::new((*self).soft_encode(store)))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprFamily<Kind = Stable<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K>
        EncodeOwned for Box<R>
    where
        Self: ReprFamily<Kind = Unstable>,
        R: Wide<Data: CheckedTransmute<CType: Sized> + EncodeOwned, Metadata = usize>,
    {
        type Store = Box<<R::Data as EncodeOwned>::Store>;

        fn soft_encode<'itm>(self, _store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let len = self.metadata();
            let data = R::into_non_null(self);

            if impls::impls!(<R::Data as EncodeOwned>::Store: EmptyStore) {
                return CBoxedSlice::from_raw_parts(data.cast(), len);
            }

            // FIXME: Should it convert to Owned?
            unimplemented!("Not supported yet")
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprFamily<Kind = Unstable> + SizeFamily<Kind = co3_types::size::Sized<S>>, S> EncodeOwned for Box<R>
    where
        Self: ReprFamily<Kind = Unstable>,
        R: EncodeOwned,
    {
        type Store = Box<R::Store>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CBox::from_box(Box::new((*self).soft_encode(store)))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprFamily<Kind = Unstable> + SizeFamily<Kind: Dst> + ?Sized> EncodeOwned for Box<R>
    where
        Self: ReprFamily<Kind = Unstable>,
        Self: ExternC<CType = <<R as StdToOwned>::Owned as ExternC>::CType>,
        R: StdToOwned<Owned: EncodeOwned>,
        Self: Into<<R as StdToOwned>::Owned>,
    {
        type Store = <R::Owned as EncodeOwned>::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.into().soft_encode(store)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprFamily<Kind = Stable<K>> + CheckedTransmute<CType: Copy>, K> EncodeOwned for Vec<R> {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            // TODO: Use Vec::into_non_null once available
            let mut source = core::mem::ManuallyDrop::new(self.into_boxed_slice());

            let len = source.len();
            let data = source.as_mut_ptr().cast();
            let data = unsafe { NonNull::new_unchecked(data) };

            CBoxedSlice::from_raw_parts(data, len)
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprFamily<Kind = Unstable> + EncodeOwned> EncodeOwned for Vec<R> {
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

            CBoxedSlice::from_boxed_slice(ctypes)
        }
    }

    unsafe impl<R: NicheFamily<Kind = WithoutNiche> + EncodeOwned<CType: Copy>> EncodeOwned for Option<R> {
        type Store = R::Store;

        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.map(|v| v.soft_encode(store)).into()
        }
    }
    unsafe impl<R: NicheFamily<Kind: WithNiche> + EncodeOwned<CType: Copy> + Niche> EncodeOwned for Option<R> {
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

    unsafe impl<
        R: SizeFamily<Kind = co3_types::size::Sized<K>> + EncodeOwned<CType: Copy>,
        E: SizeFamily<Kind = co3_types::size::Sized<K>> + EncodeOwned<CType: Copy>,
        K
    > EncodeOwned for Result<R, E>
    {
        type Store = Option<Result<R::Store, E::Store>>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Ok(ok) => {
                    let Result::Ok(store) = store.insert(Ok(Default::default())) else {
                        unreachable!()
                    };

                    ReprCResult::Ok(ok.soft_encode(store))
                }
                Err(err) => {
                    let Result::Err(store) = store.insert(Err(Default::default())) else {
                        unreachable!()
                    };

                    ReprCResult::Err(err.soft_encode(store))
                }
            }
        }
    }
    unsafe impl<
        R: SizeFamily<Kind = co3_types::size::Sized<NonZst>> + NicheFamily<Kind: WithNiche> + EncodeOwned + Niche,
        E: SizeFamily<Kind = co3_types::size::Sized<Zst>>,
    > EncodeOwned for Result<R, E>
    where
        Self: ExternC<CType = <R as ExternC>::CType>,
    {
        type Store = R::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Ok(ok) => ok.soft_encode(store),
                Err(_) => R::NICHE_VALUE,
            }
        }
    }
    unsafe impl<
        R: SizeFamily<Kind = co3_types::size::Sized<Zst>>,
        E: SizeFamily<Kind = co3_types::size::Sized<NonZst>> + NicheFamily<Kind: WithNiche> + EncodeOwned + Niche,
    > EncodeOwned for Result<R, E>
    where
        Self: ExternC<CType = <E as ExternC>::CType>,
    {
        type Store = E::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Ok(_) => E::NICHE_VALUE,
                Err(err) => err.soft_encode(store),
            }
        }
    }
}

disjoint_impls! {
    /// Perform the conversion from [`Self::CType`] into [`Self`]
    ///
    /// # Safety
    ///
    /// If [`Self::Store`] implements [`EmptyStore`], [`DecodeOwned::soft_decode`] must not
    /// return references into the store. [`crate::soft_decode`] correctness relies on it.
    ///
    /// Prefer using [`crate::soft_decode`] whenever possible
    pub unsafe trait DecodeOwned<'d>: ExternC<CType: Sized> + Sized {
        type Store: Store + Default;

        /// Perform the conversion from [`Self::CType`] into [`Self`]
        ///
        /// Prefer using [`crate::soft_decode`] whenever possible
        ///
        /// # Safety
        ///
        /// - All conversions from a pointer must ensure pointer validity beforehand
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self>;
    }

    unsafe impl<'d, R: ExternC + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: ReprFamily<Kind = Stable<NonRobust>>,
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
    unsafe impl<'d, R: ReprFamily<Kind = Stable<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> DecodeOwned<'d> for &'d R
    where
        Self: ReprFamily<Kind = Unstable>,
        R: CheckedTransmute<CType: Wide<Metadata = usize>> + Wide<Metadata = usize>,
        <R as Wide>::Data: ExternC<CType = <<R as ExternC>::CType as Wide>::Data>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_niche() {
                return None;
            }

            let ctype_slice = unsafe {
                R::CType::from_raw_parts(source.data(), source.len())
            };

            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            let len = source.len();
            let data = source.data().cast();

            Some(unsafe { R::from_raw_parts(data, len) })
        }
    }
    unsafe impl<'d, R: ReprFamily<Kind = Unstable> + SizeFamily<Kind = co3_types::size::Sized<S>> + ToOwned<'d>, S> DecodeOwned<'d>
        for &'d R
    where
        Self: ReprFamily<Kind = Unstable>,
        R: ExternC<CType: BorrowCast<AsConst: Copy> + Copy> + Borrow<Borrowed<'d>: DecodeOwned<'d>>,
        <R as Borrow>::Borrowed<'d>: ExternC<CType = <<R as ExternC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefSizedDecodeStore<R, <R::Borrowed<'d> as DecodeOwned<'d>>::Store>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let source = borrow_cast(unsafe { source.read() });

            let value = unsafe {
                <R::Borrowed<'d> as DecodeOwned>::soft_decode(source, &mut store.store)?
            };

            Some(store.value.insert(ToOwned::to_owned(value)))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: ReprFamily<Kind = Unstable> + SizeFamily<Kind = MetaSized<SliceLike>> + StdToOwned + ?Sized>
        DecodeOwned<'d> for &'d R
    where
        Self: ReprFamily<Kind = Unstable> + ExternC<CType = CSlice<<<R as Wide>::Data as ExternC>::CType>>,

        // TODO: These bounds may not be necessary
        R: ExternC<CType: Wide<Metadata = usize>> + Wide<Metadata = usize>,
        <R as Wide>::Data: DecodeOwned<'d, CType = <<R as ExternC>::CType as Wide>::Data>,
    {
        // FIXME: This type is wrong. it's supposed to be slice like owned representation
        type Store = RefDstDecodeStore<R, Box<[<R::Data as DecodeOwned<'d>>::Store]>>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _store: &'itm mut Self::Store) -> Option<Self> {
            if source.is_niche() {
                return None;
            }

            unimplemented!()
            //let source = unsafe { source.into_rust()? };

            //*store.store = core::iter::repeat_with(Default::default)
            //    .take(source.len())
            //    .collect();

            //let owned = source
            //    .iter()
            //    .cloned()
            //    .zip(store.store.iter_mut())
            //    .map(|(item, store)| unsafe { R::Data::decode_owned(item.view(), store) })
            //    .collect::<Option<Vec<_>>>()?;

            //Some(core::borrow::Borrow::borrow(store.value.insert(owned)))
        }
    }

    unsafe impl<'d, R: ExternC + ?Sized> DecodeOwned<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = Stable<NonRobust>>,
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
    unsafe impl<'d, R: ReprFamily<Kind = Stable<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> DecodeOwned<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = Unstable>,
        R: CheckedTransmute<CType: Wide<Metadata = usize>> + Wide<Metadata = usize>,
        <R as Wide>::Data: ExternC<CType = <<R as ExternC>::CType as Wide>::Data>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_niche() {
                return None;
            }

            let ctype_slice = unsafe {
                R::CType::from_raw_parts(source.data(), source.len())
            };

            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            let len = source.len();
            let data = source.data().cast();

            Some(unsafe { R::from_raw_parts_mut(data, len) })
        }
    }
    unsafe impl<'d, R: ReprFamily<Kind = Unstable> + SizeFamily<Kind = co3_types::size::Sized<S>> + ToOwned<'d>, S> DecodeOwned<'d>
        for &'d mut R
    where
        Self: ReprFamily<Kind = Unstable>,
        R: ExternC<CType: BorrowCast<AsConst: Copy> + Copy> + Borrow<Borrowed<'d>: DecodeOwned<'d>>,
        <R as Borrow>::Borrowed<'d>: ExternC<CType = <<R as ExternC>::CType as BorrowCast>::AsConst>,
    {
        type Store =
            RefMutSizedDecodeStore<R, <R::Borrowed<'d> as DecodeOwned<'d>>::Store>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let source = borrow_cast(unsafe { source.read() });

            let value = unsafe {
                <R::Borrowed<'d> as DecodeOwned>::soft_decode(source, &mut store.store)?
            };
            Some(store.value.insert(ToOwned::to_owned(value)))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: ReprFamily<Kind = Unstable> + SizeFamily<Kind = MetaSized<SliceLike>> + StdToOwned + ?Sized>
        DecodeOwned<'d> for &'d mut R
    where
        Self: ReprFamily<Kind = Unstable> + ExternC<CType: Sized>,
        R: Wide<Metadata = usize>,
        <R as Wide>::Data: DecodeOwned<'d>,
    {
        type Store = RefMutDstDecodeStore<R, Box<[<R::Data as DecodeOwned<'d>>::Store]>>;

        unsafe fn soft_decode<'itm: 'd>(_: Self::CType, _: &'itm mut Self::Store) -> Option<Self> {
            unimplemented!()
            //let value = unsafe { R::Owned::decode_owned(source, &mut store.store)? };

            //Some(core::borrow::BorrowMut::borrow_mut(
            //    store.value.insert(value),
            //))
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: CheckedTransmute<CType: Sized>> DecodeOwned<'d> for Box<R>
    where
        Self: ReprFamily<Kind = Stable<NonRobust>>,
        Self: CheckedTransmute<CType = CBox<<R as ExternC>::CType>>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            let ptr = source.data.cast();
            Some(unsafe { Box::from_raw(ptr) })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: ReprFamily<Kind = Stable<K>> + SizeFamily<Kind = MetaSized<SliceLike>> + ?Sized, K> DecodeOwned<'d> for Box<R>
    where
        Self: ReprFamily<Kind = Unstable>,
        R: CheckedTransmute<CType: Wide<Metadata = usize>> + Wide<Metadata = usize>,
        <R as Wide>::Data: ExternC<CType = <<R as ExternC>::CType as Wide>::Data>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_niche() {
                return None;
            }

            let ctype_slice = unsafe {
                R::CType::from_raw_parts(source.data(), source.len())
            };

            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            let len = source.len();
            let data = source.data().cast();

            Some(unsafe { R::from_non_null(NonNull::new_unchecked(data), len) })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: ReprFamily<Kind = Unstable> + SizeFamily<Kind = co3_types::size::Sized<S>>, S> DecodeOwned<'d>
        for Box<R>
    where
        Self: ReprFamily<Kind = Unstable>,
        R: DecodeOwned<'d, CType: Sized>,
    {
        type Store = Box<R::Store>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            let source = unsafe { source.read() };

            let value = unsafe {
                R::soft_decode(source, &mut **store)?
            };

            Some(Box::new(value))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: ReprFamily<Kind = Unstable> + SizeFamily<Kind: Dst> + StdToOwned + ?Sized>
        DecodeOwned<'d> for Box<R>
    where
        Self: ReprFamily<Kind = Unstable> + ExternC<CType = <<R as StdToOwned>::Owned as ExternC>::CType>,
        <R as StdToOwned>::Owned: DecodeOwned<'d> + Into<Self>,
    {
        type Store = <R::Owned as DecodeOwned<'d>>::Store;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { R::Owned::soft_decode(source, store) }.map(Into::into)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: ReprFamily<Kind = Stable<K>> + ExternC<CType: Copy>, K> DecodeOwned<'d> for Vec<R>
    where
        [R]: CheckedTransmute<CType: Wide<Data = <R as ExternC>::CType, Metadata = usize>>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_niche() {
                return None;
            }

            let ctype_slice = unsafe {
                <<[R] as ExternC>::CType as Wide>::from_raw_parts(source.data(), source.len())
            };

            if unsafe { !<[R]>::is_valid(ctype_slice) } {
                return None;
            }

            let len = source.len();
            let data = source.data().cast();
            // TODO: Use Box::from_non_null once available
            Some(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(data, len)) }.into())
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: ReprFamily<Kind = Unstable> + DecodeOwned<'d>> DecodeOwned<'d> for Vec<R> {
        type Store = Box<[R::Store]>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_rust()? };

            let mut is_valid = true;
            *store = core::iter::repeat_with(Default::default)
                .take(source.len())
                .collect();

            let mut decoded = Vec::with_capacity(source.len());
            for (item, store) in source.into_vec().into_iter().zip(store.iter_mut()) {
                if let Some(item) = unsafe { R::soft_decode(item, store) } {
                    decoded.push(item);
                } else {
                    is_valid = false;
                }
            }

            is_valid.then_some(decoded)
        }
    }

    unsafe impl<'d, R: NicheFamily<Kind = WithoutNiche> + DecodeOwned<'d>> DecodeOwned<'d>
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
    unsafe impl<'d, R: NicheFamily<Kind: WithNiche> + DecodeOwned<'d> + Niche<CType: PartialEq>>
        DecodeOwned<'d> for Option<R>
    where
        <Self as ExternC>::CType: Copy,
    {
        type Store = <R as DecodeOwned<'d>>::Store;

        unsafe fn soft_decode<'itm: 'd>(source: R::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source == R::NICHE_VALUE {
                return Some(None);
            }

            unsafe { R::soft_decode(source, store) }.map(Some)
        }
    }

    unsafe impl<
        'd,
        R: SizeFamily<Kind = co3_types::size::Sized<K>> + DecodeOwned<'d, CType: Copy>,
        E: SizeFamily<Kind = co3_types::size::Sized<K>> + DecodeOwned<'d, CType: Copy>,
        K
    >
        DecodeOwned<'d> for Result<R, E>
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
    unsafe impl<
        'd,
        R: SizeFamily<Kind = co3_types::size::Sized<NonZst>> + NicheFamily<Kind: WithNiche> + DecodeOwned<'d> + Niche<CType: PartialEq>,
        E: SizeFamily<Kind = co3_types::size::Sized<Zst>> + Default,
    >
        DecodeOwned<'d> for Result<R, E>
    where
        Self: ExternC<CType = <R as ExternC>::CType>,
    {
        type Store = R::Store;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source == R::NICHE_VALUE {
                return Some(Err(E::default()));
            }

            unsafe { R::soft_decode(source, store) }.map(Ok)
        }
    }
    unsafe impl<
        'd,
        R: SizeFamily<Kind = co3_types::size::Sized<Zst>> + Default,
        E: SizeFamily<Kind = co3_types::size::Sized<NonZst>> + NicheFamily<Kind: WithNiche> + DecodeOwned<'d> + Niche<CType: PartialEq>,
    >
        DecodeOwned<'d> for Result<R, E>
    where
        Self: ExternC<CType = <E as ExternC>::CType>,
    {
        type Store = E::Store;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            if source == E::NICHE_VALUE {
                return Some(Ok(R::default()));
            }

            unsafe { E::soft_decode(source, store) }.map(Err)
        }
    }
}

/// Perform the conversion from `T` into [`T::CType`] using external storage.
///
/// Prefer using [`crate::encode`] whenever possible.
pub(crate) fn encode_owned<T: EncodeOwned<Store: EmptyStore>>(item: T) -> T::CType {
    let mut store = Default::default();
    T::soft_encode(item, &mut store)
}

/// Perform the conversion from [`T::CType`](crate::ExternC::CType) into `T` without external storage.
///
/// Prefer using [`crate::decode`] whenever possible.
///
/// # Safety
///
/// - All conversions from a pointer must ensure pointer validity beforehand
pub(crate) unsafe fn decode_owned<'d, T: DecodeOwned<'d, Store: EmptyStore + 'd>>(
    source: T::CType,
) -> Option<T> {
    unsafe fn extend_store_lifetime<'d, S>(store: &mut S) -> &'d mut S {
        unsafe { core::mem::transmute::<&mut S, &'d mut S>(store) }
    }

    let mut store = T::Store::default();
    // SAFETY: `decode_owned` is only available for zero-sized stores, so extending
    // the borrow of the local store does not extend the lifetime of any backing data.
    let store = unsafe { extend_store_lifetime(&mut store) };
    unsafe { T::soft_decode(source, store) }
}

impl Store for () {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

#[cfg(feature = "alloc")]
impl<D: Store> Store for Box<D> {
    fn sync(self) -> Option<()> {
        (*self).sync()
    }
}

#[cfg(feature = "alloc")]
impl<D: Store> Store for Box<[D]> {
    fn sync(self) -> Option<()> {
        let mut is_valid = true;

        for store in self {
            if store.sync().is_none() {
                is_valid = false;
            }
        }

        is_valid.then_some(())
    }
}

pub struct RefSizedEncodeStore<R: EncodeOwned> {
    pub(crate) ctype: Option<R::CType>,
    pub(crate) store: R::Store,
}

#[cfg(feature = "alloc")]
pub struct RefDstEncodeStore<R: StdToOwned<Owned: EncodeOwned> + ?Sized> {
    pub(crate) ctype: Option<<R::Owned as ExternC>::CType>,
    pub(crate) store: <R::Owned as EncodeOwned>::Store,
}

pub struct RefMutSizedEncodeStore<'d, R: EncodeOwned> {
    pub(crate) ctype: Option<R::CType>,
    pub(crate) store: R::Store,
    pub(crate) original: Option<&'d mut R>,
}

#[cfg(feature = "alloc")]
pub struct RefMutDstEncodeStore<'d, R: StdToOwned<Owned: EncodeOwned> + ?Sized> {
    pub(crate) ctype: Option<<R::Owned as ExternC>::CType>,
    pub(crate) store: <R::Owned as EncodeOwned>::Store,
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

impl<R: EncodeOwned> Default for RefSizedEncodeStore<R> {
    fn default() -> Self {
        Self {
            ctype: None,
            store: Default::default(),
        }
    }
}

impl<R: EncodeOwned> Store for RefSizedEncodeStore<R> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned<Owned: EncodeOwned> + ?Sized> Default for RefDstEncodeStore<R> {
    fn default() -> Self {
        Self {
            ctype: None,
            store: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: StdToOwned<Owned: EncodeOwned> + ?Sized> Store for RefDstEncodeStore<R> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

impl<'d, R: EncodeOwned> Default for RefMutSizedEncodeStore<'d, R> {
    fn default() -> Self {
        Self {
            ctype: Default::default(),
            store: Default::default(),
            original: Default::default(),
        }
    }
}

// &mut &(u32,)
// TODO: I'd bet the store doesn't have to be empty on decode during sync
// it should be possible to traverse the ctype and update current type.
impl<'d, R: EncodeOwned + DecodeOwned<'d>> Store for RefMutSizedEncodeStore<'d, R> {
    fn sync(self) -> Option<()> {
        // FIXME:
        //const {
        //    assert!(co3::impls!(R: Decode<'static>), "Not yet implemented");
        //}

        unimplemented!()
        //self.store.sync()?;

        //if let (Some(ctype), Some(original)) = (self.ctype, self.original) {
        //    *original = unsafe { decode_owned(ctype)? };
        //}

        //Some(())
    }
}

#[cfg(feature = "alloc")]
impl<'d, R: StdToOwned<Owned: EncodeOwned> + ?Sized> Default for RefMutDstEncodeStore<'d, R> {
    fn default() -> Self {
        Self {
            ctype: Default::default(),
            store: Default::default(),
            original: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<'d, 'b, R: StdToOwned<Owned: EncodeOwned + DecodeOwned<'b>> + ?Sized> Store
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
        let mut is_valid = true;

        for store in self.0 {
            if store.sync().is_none() {
                is_valid = false;
            }
        }

        is_valid.then_some(())
    }
}

unsafe impl<D: EmptyStore, const N: usize> EmptyStore for ArrayStore<D, N> {}

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
