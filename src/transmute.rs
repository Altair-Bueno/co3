#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use co3_types::size::{NonZst, SizeFamily, Zst};
use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::boxed::CBox;
use crate::{ExternC, assert_arr_has_non_zero_len, niche::StableNiche};

disjoint_impls! {
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

    unsafe impl<R: CheckedTransmute<CType: Copy> + StableNiche, E> CheckedTransmute for Result<R, E>
    where
        R: SizeFamily<Kind = co3_types::size::Sized<NonZst>>,
        E: SizeFamily<Kind = co3_types::size::Sized<Zst>>,
        Self: ExternC<CType = <R as ExternC>::CType>,
    {
        #[inline(always)]
        unsafe fn is_valid(target: &Self::CType) -> bool {
            unsafe { R::is_valid(target) }
        }
    }
    // FIXME: disjoint_impls! is broken
    //unsafe impl<R, E: CheckedTransmute<CType: Copy> + StableNiche> CheckedTransmute for Result<R, E>
    //where
    //    R: SizeFamily<Kind = co3_types::size::Sized<Zst>>,
    //    E: SizeFamily<Kind = co3_types::size::Sized<NonZst>>,
    //    Self: ExternC<CType = <E as ExternC>::CType>,
    //{
    //    #[inline(always)]
    //    unsafe fn is_valid(target: &Self::CType) -> bool {
    //        unsafe { E::is_valid(target) }
    //    }
    //}
}

unsafe impl<R: CheckedTransmute + ?Sized> CheckedTransmute for &R
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

// FIXME: Should it be implemented for non-robust R?
// atm we say yes, this is transmutable but don't misuse it.
// Either require Stable<Robust> or write this in the documentation
// If Repr<Robust> then also consider how it affects Box<&mut R>
unsafe impl<R: CheckedTransmute + ?Sized> CheckedTransmute for &mut R
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
        if target.is_niche() {
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

unsafe impl<R: CheckedTransmute<CType: Copy>> CheckedTransmute for [R] {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        for item in target {
            if unsafe { !R::is_valid(item) } {
                return false;
            }
        }

        true
    }
}

unsafe impl CheckedTransmute for str {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}

unsafe impl<R: CheckedTransmute<CType: Copy> + StableNiche> CheckedTransmute for Option<R>
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
    use alloc::vec::Vec;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::CBoxedSlice;
    use crate::{
        Decode, Encode, ReprC,
        niche::Niche,
        slice::{CSlice, CSliceMut},
    };

    #[test]
    fn transparent_type() {
        assert_impl_all!(bool:
            Niche<CType = u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&bool:
            StableNiche<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut bool:
            StableNiche<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<bool>:
            StableNiche<CType = CBox<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[bool]:
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [bool]:
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[bool]>:
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<bool>:
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([bool; 2]:
            Niche<CType = [u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<bool>:
            Niche<CType = u8>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn robust_ref() {
        assert_impl_all!(&&u8:
            StableNiche<CType = *const *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &u8:
            StableNiche<CType = *mut *const u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            StableNiche<CType = CBox<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&u8]:
            Niche<CType = CSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&u8]:
            Niche<CType = CSliceMut<*const u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&u8]>:
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&u8>:
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&u8; 2]:
            Niche<CType = [*const u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&u8>:
            ExternC<CType = *const u8>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(Option<&u8>: ReprC);
    }

    #[test]
    fn transparent_ref() {
        assert_impl_all!(&&bool:
            StableNiche<CType = *const *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &bool:
            StableNiche<CType = *mut *const u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            StableNiche<CType = CBox<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&bool]:
            Niche<CType = CSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [&bool]:
            Niche<CType = CSliceMut<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&bool]>:
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&bool>:
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&bool; 2]:
            Niche<CType = [*const u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&bool>:
            ExternC<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn robust_ref_mut() {
        assert_impl_all!(&&mut u8:
            StableNiche<CType = *const *mut u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut u8:
            StableNiche<CType = *mut *mut u8>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut u8>:
            StableNiche<CType = CBox<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut u8]:
            Niche<CType = CSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut u8]:
            Niche<CType = CSliceMut<*mut u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut u8]>:
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut u8>:
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&mut u8; 2]:
            Niche<CType = [*mut u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&mut u8>:
            ExternC<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(Option<&mut u8>: ReprC);
    }

    #[test]
    fn transparent_ref_mut() {
        assert_impl_all!(&&mut bool:
            StableNiche<CType = *const *mut u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut bool:
            StableNiche<CType = *mut *mut u8>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut bool>:
            StableNiche<CType = CBox<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut bool]:
            Niche<CType = CSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut bool]:
            Niche<CType = CSliceMut<*mut u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut bool]>:
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut bool>:
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!([&mut bool; 2]:
            Niche<CType = [*mut u8; 2]>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!(Option<&mut bool>:
            ExternC<CType = *mut u8>,
            Decode<'static>,
            Encode
        );
    }
}
