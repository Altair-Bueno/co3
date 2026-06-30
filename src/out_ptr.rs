#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};

use crate::{CFnReturn, ExternC};

/// Marker for a ZST(zero-sized type)
///
/// # Safety
///
/// Type must be a ZST
pub unsafe trait Zst {}

unsafe impl Zst for () {}
unsafe impl<T: Zst> Zst for [T] {}
#[cfg(feature = "alloc")]
unsafe impl<T: Zst> Zst for Vec<T> {}
unsafe impl<T: Zst> Zst for Option<T> {}
#[cfg(feature = "alloc")]
unsafe impl<T: Zst + ?Sized> Zst for Box<T> {}
unsafe impl<T: Zst, E: Zst> Zst for Result<T, E> {}
unsafe impl<T: Zst, const N: usize> Zst for [T; N] {}
// TODO: It's not possbile to implement for specific len yet: https://github.com/mversic/co3/issues/13
//unsafe impl<T> Zst for [T; 0] {}
unsafe impl<T> Zst for core::marker::PhantomData<T> {}
unsafe impl<T: Zst> Zst for core::mem::ManuallyDrop<T> {}
unsafe impl<T: Zst> Zst for core::cell::UnsafeCell<T> {}

pub trait OutPtr: ExternC {
    type OutPtr: CFnReturn;
}

impl<R: ExternC> OutPtr for R
where
    R::CType: CFnReturn,
{
    type OutPtr = R::CType;
}

pub trait OutPtrWrite: OutPtr {
    /// # Safety
    ///
    /// `out_ptr` must be valid for writes of `Self::OutPtr`.
    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr);
}

impl<R> OutPtrWrite for R
where
    R: OutPtr<OutPtr = <R as ExternC>::CType> + crate::stored::EncodeOwned,
    R::Store: Default,
{
    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
        let mut store = Default::default();
        let encoded = crate::stored::EncodeOwned::soft_encode_owned(self, &mut store);

        unsafe { out_ptr.write(encoded) };
    }
}

//disjoint_impls! {
//    /// Facilitates the use of [`Self`] as out-pointer.
//    ///
//    /// If a type implements [`Repr`], i.e. has a defined internal representation,
//    /// a blanket implementation is provided.
//    pub trait OutPtr: ExternC {
//        /// Type of the out-pointer
//        type OutPtr: RobustReprC;
//    }
//
//    impl<R: RobustReprC> OutPtr for R
//    where
//        Self: ReprFamily<Kind = Robust>,
//    {
//        type OutPtr = Self::CType;
//    }
//    impl<R: CheckedTransmute<Target: Sized>> OutPtr for R
//    where
//        Self: ReprFamily<Kind = ReprC>,
//        <R as CheckedTransmute>::Target: OutPtr,
//    {
//        type OutPtr = <R::Target as OutPtr>::OutPtr;
//    }
//
//    impl<'a, R: SizeFamily<Kind = SliceLike> + SliceDst<Elem: RobustReprC> + ?Sized> OutPtr for &'a R
//    where
//        Self: ReprFamily<Kind = &'a Robust>,
//    {
//        type OutPtr = Self::CType;
//    }
//    //impl<'a, R: ?Sized> OutPtr for &'a R
//    //where
//    //    Self: ReprFamily<Kind = &'a Opaque>,
//    //{
//    //    type OutPtr = CBoxedSlice<*const R>;
//    //}
//    impl<'a, R: CheckedTransmute + ?Sized> OutPtr for &'a R
//    where
//        &'a <R as CheckedTransmute>::Target: OutPtr,
//        Self: ReprFamily<Kind = &'a RobustReprC>,
//    {
//        type OutPtr = <&'a R::Target as OutPtr>::OutPtr;
//    }
//    impl<'a, R: ExternC + Stored<Kita = *const R::CType>> OutPtr for &'a R
//    where
//        Self: ReprFamily<Kind = ReprRust>,
//        R: SizeFamily<Kind: Sized_>,
//    {
//        type OutPtr = R::CType;
//    }
//    impl<
//        'a,
//        R: SliceDst<Elem: OutPtr> + Stored<Kita = CSlice<<R::Elem as ExternC>::CType>> + ?Sized,
//    > OutPtr for &'a R
//    where
//        Self: ReprFamily<Kind = ReprRust>,
//        R: SizeFamily<Kind = SliceLike>,
//    {
//        type OutPtr = CSlice<<R::Elem as OutPtr>::OutPtr>;
//    }
//
//    impl<'a, R: SizeFamily<Kind = SliceLike> + SliceDst<Elem: RobustReprC> + ?Sized> OutPtr for &'a mut R
//    where
//        Self: ReprFamily<Kind = &'a mut Robust>,
//    {
//        type OutPtr = Self::CType;
//    }
//    impl<'a, R: CheckedTransmute + ?Sized> OutPtr for &'a mut R
//    where
//        &'a mut <R as CheckedTransmute>::Target: OutPtr,
//        Self: ReprFamily<Kind = &'a mut RobustReprC>,
//        R: SizeFamily<Kind = SliceLike>,
//    {
//        type OutPtr = <&'a mut R::Target as OutPtr>::OutPtr;
//    }
//
//    #[cfg(feature = "alloc")]
//    impl<R: SizeFamily<Kind = SliceLike> + SliceDst<Elem: RobustReprC> + ?Sized> OutPtr for Box<R>
//    where
//        Self: ReprFamily<Kind = ReprRust>,
//    {
//        type OutPtr = Self::CType;
//    }
//    #[cfg(feature = "alloc")]
//    impl<R: CheckedTransmute + ?Sized> OutPtr for Box<R>
//    where
//        Box<<R as CheckedTransmute>::Target>: OutPtr,
//        Self: ReprFamily<Kind = ReprRust>,
//        R: SizeFamily<Kind = SliceLike>,
//    {
//        type OutPtr = <Box<R::Target> as OutPtr>::OutPtr;
//    }
//    //#[cfg(feature = "alloc")]
//    //impl<R: Dst + ?Sized> OutPtr for Box<R>
//    //where
//    //    Self: ReprFamily<Kind = ReprRust>,
//    //{
//    //    type OutPtr = CBoxedSlice<CBox<R>>;
//    //}
//    #[cfg(feature = "alloc")]
//    impl<R: ExternC + Stored<Kita = CBox<R::CType>>> OutPtr for Box<R>
//    where
//        Self: ReprFamily<Kind = ReprRust>,
//        R: SizeFamily<Kind: Sized_>,
//    {
//        type OutPtr = R::CType;
//    }
//    #[cfg(feature = "alloc")]
//    impl<
//        R: SliceDst<Elem: OutPtr>
//            + Stored<Kita = CBoxedSlice<<R::Elem as ExternC>::CType>> + ?Sized,
//    > OutPtr for Box<R>
//    where
//        Self: ReprFamily<Kind = ReprRust>,
//        R: SizeFamily<Kind = SliceLike>,
//    {
//        type OutPtr = CBoxedSlice<<R::Elem as OutPtr>::OutPtr>;
//    }
//
//    #[cfg(feature = "alloc")]
//    impl<R, S> OutPtr for Vec<R>
//    where
//        Self: ReprFamily<Kind = ReprRust>,
//        Box<[R]>: OutPtr,
//    {
//        type OutPtr = <Box<[R]> as OutPtr>::OutPtr;
//    }
//
//    impl<R: ExternC + Stored<Kita = [R::CType; N]>, const N: usize> OutPtr for [R; N]
//    where
//        Self: ReprFamily<Kind = ReprRust>,
//    {
//        type OutPtr = Self::CType;
//    }
//
//    impl<R: OutPtr> OutPtr for Option<R>
//    where
//        Self: ReprFamily<Kind = ReprRust> + NicheFamily<Kind = WithoutNiche>,
//    {
//        type OutPtr = COption<R::OutPtr>;
//    }
//    impl<R: Niche + OutPtr> OutPtr for Option<R>
//    where
//        Self: ReprFamily<Kind = ReprRust> + NicheFamily<Kind = WithCustomNiche>,
//    {
//        type OutPtr = R::OutPtr;
//    }
//}
//
//disjoint_impls! {
//    /// Facilitates writing [`Self`] into [`Self::OutPtr`].
//    pub trait OutPtrWrite: OutPtr {
//        /// Write the given rust value into the corresponding out-pointer
//        ///
//        /// # Safety
//        ///
//        /// [`*mut Self::OutPtr`] must be valid
//        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr);
//    }
//
//    impl<R: RobustReprC> OutPtrWrite for R
//    where
//        Self: ReprFamily<Kind = Robust>,
//    {
//        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//            let ctype = Encode::encode_owned(self, &mut ());
//
//            unsafe {
//                out_ptr.write(ctype);
//            }
//        }
//    }
//    impl<R: CheckedTransmute<Target: Sized>> OutPtrWrite for R
//    where
//        Self: ReprFamily<Kind = ReprC>,
//        <R as CheckedTransmute>::Target: OutPtrWrite,
//    {
//        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//            let transmuted = transmute_into_target(self);
//            unsafe { OutPtrWrite::write_out(transmuted, out_ptr) }
//        }
//    }
//
//    //impl<'itm, R: Encode + ToOwned<Owned = R>, S: Stored> OutPtrWrite for &'itm R
//    //where
//    //    Self: ReprFamily<Kind = &'itm S>,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        let mut store = Default::default();
//    //        let _ = self.soft_encode_owned(&mut store);
//    //        let output = store.ctype.unwrap();
//
//    //        unsafe {
//    //            out_ptr.write(output);
//    //        }
//    //    }
//    //}
//
//    //#[cfg(feature = "alloc")]
//    //impl<R: Encode, S: Stored> OutPtrWrite for Box<R>
//    //where
//    //    Self: ReprFamily<Kind = ReprRust>,
//    //{
//    //    unsafe fn write_out(self, _out_ptr: *mut Self::OutPtr) {
//    //        unimplemented!()
//    //        //let mut store = Default::default();
//    //        //let _ = self.soft_encode_owned(&mut store);
//    //        //let output = store.ctype.unwrap();
//
//    //        //unsafe {
//    //        //    out_ptr.write(output);
//    //        //}
//    //    }
//    //}
//
//    //impl<'a, R: Dst<Data: RobustReprC> + ?Sized> OutPtrWrite for &'a R
//    //where
//    //    Self: ReprFamily<Kind = &'a Robust>,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        let ctypes = self.soft_encode_owned(&mut ());
//
//    //        unsafe {
//    //            out_ptr.write(ctypes);
//    //        }
//    //    }
//    //}
//    //impl<R: ?Sized> OutPtrWrite for &R
//    //where
//    //    Self: ReprFamily<Kind = Opaque>,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        let mut store = Default::default();
//    //        let _ = self.soft_encode_owned(&mut store);
//
//    //        let output = CBoxedSlice::from_boxed_slice(store.0);
//
//    //        unsafe {
//    //            out_ptr.write(output);
//    //        }
//    //    }
//    //}
//    //impl<'a, R: CheckedTransmute<Target: Sized + 'a>> OutPtrWrite for &'a R
//    //where
//    //    &'a <R as CheckedTransmute>::Target: OutPtrWrite,
//    //    Self: ReprFamily<Kind = &'a RobustReprC>,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        let transmuted = transmute_into_target_ref_slice(self);
//
//    //        unsafe {
//    //            OutPtrWrite::write_out(transmuted, out_ptr);
//    //        }
//    //    }
//    //}
//    //#[soft]
//    //impl<R: Encode + ToOwned<Owned = R>, S: Stored> OutPtrWrite for &R
//    //where
//    //    Self: ReprFamily<Kind = S>,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        let mut store = Default::default();
//    //        let _ = self.soft_encode_owned(&mut store);
//
//    //        let output = CBoxedSlice::from_boxed_slice(store.ctypes);
//
//    //        unsafe {
//    //            out_ptr.write(output);
//    //        }
//    //    }
//    //}
//
//    //impl<'a, R: CheckedTransmute<Target: Sized + 'a>> OutPtrWrite for &'a mut R
//    //where
//    //    &'a mut <R as CheckedTransmute>::Target: OutPtrWrite,
//    //    Self: ReprFamily<Kind = &'a mut RobustReprC>,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        let transmuted = transmute_into_target_slice_mut(self);
//
//    //        unsafe {
//    //            OutPtrWrite::write_out(transmuted, out_ptr);
//    //        }
//    //    }
//    //}
//
//    //#[cfg(feature = "alloc")]
//    //impl<R: Dst<Data: RobustReprC> + ?Sized> OutPtrWrite for Box<R>
//    //where
//    //    Self: ReprFamily<Kind = ReprRust>,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        let output = self.soft_encode_owned(&mut ());
//
//    //        unsafe {
//    //            out_ptr.write(output);
//    //        }
//    //    }
//    //}
//    //#[cfg(feature = "alloc")]
//    //impl<R: ?Sized> OutPtrWrite for Box<R>
//    //where
//    //    Self: ReprFamily<Kind = Opaque>,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        let output = self.soft_encode_owned(&mut ());
//
//    //        unsafe {
//    //            out_ptr.write(output);
//    //        }
//    //    }
//    //}
//    //#[cfg(feature = "alloc")]
//    //impl<R: CheckedTransmute<Target: Sized>> OutPtrWrite for Box<R>
//    //where
//    //    Self: ReprFamily<Kind = ReprRust>,
//    //    Box<<R as CheckedTransmute>::Target>: OutPtrWrite,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        let transmuted = transmute_into_target_boxed_slice(self);
//
//    //        unsafe {
//    //            OutPtrWrite::write_out(transmuted, out_ptr);
//    //        }
//    //    }
//    //}
//    //#[cfg(feature = "alloc")]
//    //impl<R: Encode, S: Stored> OutPtrWrite for Box<R>
//    //where
//    //    Self: ReprFamily<Kind = S>,
//    //{
//    //    unsafe fn write_out(self, _out_ptr: *mut Self::OutPtr) {
//    //        unimplemented!()
//    //        //let mut store = Default::default();
//    //        //let _ = self.soft_encode_owned(&mut store);
//
//    //        //let output = CBoxedSlice::from_boxed_slice(store.ctypes);
//
//    //        //unsafe {
//    //        //    out_ptr.write(output);
//    //        //}
//    //    }
//    //}
//
//    //#[cfg(feature = "alloc")]
//    //impl<R, S> OutPtrWrite for Vec<R>
//    //where
//    //    Self: ReprFamily<Kind = ReprRust>,
//    //    Box<[R]>: OutPtrWrite,
//    //{
//    //    unsafe fn write_out(self, _out_ptr: *mut Self::OutPtr) {
//    //        unimplemented!()
//    //    }
//    //}
//
//    //impl<R: Encode, S: Stored, const N: usize> OutPtrWrite for [R; N]
//    //where
//    //    Self: ReprFamily<Kind = [S; N]> + Encode,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        assert_arr_has_non_zero_len::<N>();
//
//    //        let mut store = Default::default();
//    //        let item = self.soft_encode_owned(&mut store);
//
//    //        unsafe {
//    //            out_ptr.write(item);
//    //        }
//    //    }
//    //}
//
//    //impl<R: OutPtrWrite> OutPtrWrite for Option<R>
//    //where
//    //    Self: ReprFamily<Kind = ReprRust>,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        match self {
//    //            None => unsafe { out_ptr.write(COption::None()) },
//    //            Some(value) => unsafe {
//    //                let mut value_out_ptr = core::mem::MaybeUninit::uninit();
//    //                OutPtrWrite::write_out(value, value_out_ptr.as_mut_ptr());
//    //                let value_out_ptr = value_out_ptr.assume_init();
//
//    //                out_ptr.write(COption::Some(value_out_ptr));
//    //            },
//    //        }
//    //    }
//    //}
//    //impl<R: Niche + OutPtrWrite<OutPtr = <R as ExternC>::CType>> OutPtrWrite for Option<R>
//    //where
//    //    Self: ReprFamily<Kind = ReprRust>,
//    //{
//    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//    //        self.map_or_else(
//    //            || unsafe { out_ptr.write(R::NICHE_VALUE) },
//    //            |v| unsafe { OutPtrWrite::write_out(v, out_ptr) },
//    //        );
//    //    }
//    //}
//}
//
////#[cfg(test)]
////mod tests {
////    #[cfg(feature = "alloc")]
////    use static_assertions::assert_impl_all;
////
////    use super::*;
////
////    #[test]
////    #[cfg(feature = "alloc")]
////    fn non_local_types() {
////        //#[cfg(feature = "owned-as-ref")]
////        //{
////        //    // FIXME:
////        //    //assert_not_impl_any!(Vec<u8>: OutPtrWrite);
////        //    assert_not_impl_any!(Option<Vec<u8>>: OutPtrWrite);
////        //    assert_not_impl_any!(&Vec<u8>: OutPtrWrite);
////        //    assert_not_impl_any!(&Option<Vec<u8>>: OutPtrWrite);
////        //}
////
////        {
////            assert_impl_all!(Vec<u8>: OutPtrWrite);
////            assert_impl_all!(Option<Vec<u8>>: OutPtrWrite);
////            assert_impl_all!(&Vec<u8>: OutPtrWrite);
////            assert_impl_all!(&Option<Vec<u8>>: OutPtrWrite);
////        }
////    }
////
////    // TODO:
////    //#[test]
////    //pub fn nested_owned() {
////    //    unimplemented!()
////    //}
////}
