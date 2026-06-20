#[cfg(feature = "alloc")]
use alloc_crate::boxed::Box;

#[cfg(feature = "alloc")]
use crate::boxed::CBox;
use crate::{ExternC, assert_arr_has_non_zero_len, niche::StableNiche};

/// Type that can be **safely transmuted** into another type.
///
/// # Safety
///
/// - `Self` and `Self::CType` must be mutually transmutable (this includes [`Drop`] semantics)
/// - `Self::is_valid` must not return false positives, i.e. return `true` for trap representations
pub unsafe trait CheckedTransmute: ExternC {
    /// Called when transmuting [`Self::CType`] back into [`Self`] to check for trap representations.
    ///
    /// This function must never return false positives, i.e. return `true` for a trap representation.
    ///
    /// # Safety
    ///
    /// pointers in `Self::Target` must be valid for reads
    unsafe fn is_valid(target: &Self::CType) -> bool;
}

unsafe impl<R: ExternC + CheckedTransmute + ?Sized> CheckedTransmute for &R
where
    Self: ExternC<CType = *const R::CType>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        if target.is_null() {
            return false;
        }

        unsafe { R::is_valid(&**target) }
    }
}

unsafe impl<R: ExternC + CheckedTransmute + ?Sized> CheckedTransmute for &mut R
where
    Self: ExternC<CType = *mut R::CType>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        if target.is_null() {
            return false;
        }

        unsafe { R::is_valid(&**target) }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R: CheckedTransmute<CType: Sized>> CheckedTransmute for Box<R>
where
    Self: ExternC<CType = CBox<R::CType>>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        if target.is_none() {
            return false;
        }

        unsafe { R::is_valid(&*target.data) }
    }
}

unsafe impl<R: CheckedTransmute<CType: Copy>, const N: usize> CheckedTransmute for [R; N] {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        assert_arr_has_non_zero_len::<N>();

        for t in target {
            if unsafe { !R::is_valid(t) } {
                return false;
            }
        }

        true
    }
}

unsafe impl<R: StableNiche + CheckedTransmute> CheckedTransmute for Option<R>
where
    Self: ExternC<CType = R::CType>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { R::is_valid(target) }
    }
}

// TODO: Use this somehow to strengthen CheckedTransmute assumptions
//fn assert_size_and_allignment_match<R: CheckedTransmute<CType: Copy>>() {
//    const {
//        debug_assert!(core::mem::size_of::<R>() == core::mem::size_of::<R::CType>());
//        debug_assert!(core::mem::align_of::<R>() == core::mem::align_of::<R::CType>());
//    };
//}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc_crate::vec::Vec;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::CBoxedSlice;
    use crate::{
        Decode, Encode, ReprC, SoftEncode,
        ir::{NonRobust, ReprFamily, ReprRust, Transmuted},
        niche::{Niche, NicheFamily, WithCustomNiche, WithStableNiche, WithoutNiche},
        slice::{CSlice, CSliceMut},
        stored::{EncodeOwned, SoftEncodeOwned},
    };

    #[test]
    fn transparent_type() {
        assert_impl_all!(bool:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&bool:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut bool:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            Decode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<bool>:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[bool]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [bool]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[bool]>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<bool>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([bool; 2]:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<bool>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = u8>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(&mut bool: EncodeOwned);
        assert_not_impl_any!(&mut [bool]: EncodeOwned);
    }

    #[test]
    fn robust_ref() {
        assert_impl_all!(&&u8:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &u8:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *const u8>,
            Decode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&u8]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&u8]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&u8]>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&u8>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&u8; 2]:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*const u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&u8>:
            // FIXME:
            //ReprFamily<Kind = Transmuted<Robust>>,
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *const u8>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(Option<&u8>: ReprC);

        assert_not_impl_any!(&mut &u8: EncodeOwned);
        assert_not_impl_any!(&mut [&u8]: EncodeOwned);
    }

    #[test]
    fn transparent_ref() {
        assert_impl_all!(&&bool:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &bool:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *const u8>,
            Decode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&bool]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [&bool]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const u8>>,
            Decode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&bool]>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&bool>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&bool; 2]:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*const u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&bool>:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *const u8>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(&mut &bool: EncodeOwned);
        assert_not_impl_any!(&mut [&bool]: EncodeOwned);
    }

    #[test]
    fn robust_ref_mut() {
        assert_impl_all!(&&mut u8:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *mut u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut u8:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *mut u8>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut u8>:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<*mut u8>>,
            Decode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&[&mut u8]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut u8]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*mut u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut u8]>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut u8>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&mut u8; 2]:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*mut u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&mut u8>:
            // FIXME:
            //ReprFamily<Kind = Transmuted<Robust>>,
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(Option<&mut u8>: ReprC);

        assert_not_impl_any!(&mut &mut u8: SoftEncodeOwned);
        assert_not_impl_any!(&mut [&mut u8]: SoftEncodeOwned);
    }

    #[test]
    fn transparent_ref_mut() {
        assert_impl_all!(&&mut bool:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *mut u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut bool:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *mut u8>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut bool>:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<*mut u8>>,
            Decode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&[&mut bool]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut bool]:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*mut u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut bool]>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut bool>:
            ReprFamily<Kind = ReprRust>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            SoftEncode
        );
        assert_impl_all!([&mut bool; 2]:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*mut u8; 2]>,
            Decode<'static>,
            SoftEncode
        );
        assert_impl_all!(Option<&mut bool>:
            ReprFamily<Kind = Transmuted<NonRobust>>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *mut u8>,
            Decode<'static>,
            SoftEncode
        );

        assert_not_impl_any!(&mut &mut bool: SoftEncodeOwned);
        assert_not_impl_any!(&mut [&mut bool]: SoftEncodeOwned);

        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<&mut bool>: EncodeOwned);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[&mut bool]>: EncodeOwned);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Vec<&mut bool>: EncodeOwned);
        assert_not_impl_any!([&mut bool; 2]: EncodeOwned);
        assert_not_impl_any!(Option<&mut bool>: EncodeOwned);
    }
}
