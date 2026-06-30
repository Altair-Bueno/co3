//! Logic related to the conversion of slices to and from FFI-compatible representation

use crate::{
    CFnArg, Decode, Encode, ExternC, RobustReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, ToOwned},
    handle::Erase,
    ir::ReprFamily,
    niche::{NicheFamily, WithoutNiche},
    size::SizeFamily,
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
};

/// Immutable slice `&[C]` with a defined C ABI layout. Consists of a data pointer and a length.
/// If the data pointer is set to `null`, the struct represents `Option<&[C]>`.
#[repr(C)]
pub struct CSlice<C> {
    data: *const C,
    len: usize,
}

/// Mutable slice `&mut [C]` with a defined C ABI layout. Consists of a data pointer and a length.
/// If the data pointer is set to `null`, the struct represents `Option<&mut [C]>`.
#[repr(C)]
pub struct CSliceMut<C> {
    data: *mut C,
    len: usize,
}

macro_rules! impl_raw_slice_methods {
    ($($ty:ty),+ $(,)?) => {$(
        impl<C> core::fmt::Debug for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                if self.data.is_null() {
                    f.debug_struct(stringify!($ty))
                        .field("data", &self.data)
                        .finish_non_exhaustive()
                } else {
                    f.debug_struct(stringify!($ty))
                        .field("data", &self.data)
                        .field("len", &self.len)
                        .finish()
                }
            }
        }
        impl<C> PartialEq for $ty {
            fn eq(&self, other: &Self) -> bool {
                match (self.data.is_null(), other.data.is_null()) {
                    (true, true) => true,
                    (false, false) => {
                        if self.len == 0 || other.len == 0 {
                            self.len == other.len
                        } else {
                            self.data == other.data && self.len == other.len
                        }
                    }
                    _ => false,
                }
            }
        }
        impl<C> Eq for $ty {}
        impl<C> PartialOrd for $ty {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl<C> Ord for $ty {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                use core::cmp::Ordering;
                match (self.data.is_null(), other.data.is_null()) {
                    (true, true) => Ordering::Equal,
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => {
                        if self.len == 0 || other.len == 0 {
                            self.len.cmp(&other.len)
                        } else {
                            match self.data.cmp(&other.data) {
                                Ordering::Equal => self.len.cmp(&other.len),
                                ordering => ordering,
                            }
                        }
                    }
                }
            }
        }
        impl<C> Clone for $ty {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<C> Copy for $ty {})+
    };
}

impl_raw_slice_methods! { CSlice<C>, CSliceMut<C> }

impl<C> CSlice<C> {
    /// Set the slice's data pointer to null
    pub(crate) const NICHE_VALUE: Self = Self {
        data: core::ptr::null(),
        // TODO: Use MaybeUninit for len?
        len: 0,
    };

    /// Create [`Self`] from shared slice
    pub const fn from_slice(source: Option<&[C]>) -> Self {
        if let Some(slice) = source {
            return Self {
                data: slice.as_ptr(),
                len: slice.len(),
            };
        }

        Self::NICHE_VALUE
    }

    /// Create [`Self`] from a raw data pointer and slice metadata.
    pub(crate) const fn from_raw_parts(data: *const C, len: usize) -> Self {
        Self { data, len }
    }
}

impl<C> CSliceMut<C> {
    /// Set the slice's data pointer to null
    pub(crate) const NICHE_VALUE: Self = Self {
        data: core::ptr::null_mut(),
        // TODO: Use MaybeUninit for len?
        len: 0,
    };

    /// Create [`Self`] from mutable slice
    pub const fn from_slice(source: Option<&mut [C]>) -> Self {
        if let Some(slice) = source {
            return Self {
                data: slice.as_mut_ptr(),
                len: slice.len(),
            };
        }

        Self::NICHE_VALUE
    }

    /// Create [`Self`] from a raw data pointer and slice metadata.
    pub(crate) const fn from_raw_parts_mut(data: *mut C, len: usize) -> Self {
        Self { data, len }
    }
}

impl<C> CSlice<C> {
    /// Convert [`Self`] into a shared slice. Return `None` if data pointer is null.
    /// Unlike [`core::slice::from_raw_parts`], data pointer is allowed to be null.
    ///
    /// # Safety
    ///
    /// Check [`core::slice::from_raw_parts`]
    pub const unsafe fn into_rust<'slice>(self) -> Option<&'slice [C]> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { core::slice::from_raw_parts(self.data, self.len) })
    }
}

impl<C> CSliceMut<C> {
    /// Convert [`Self`] into a mutable slice. Return `None` if data pointer is null.
    /// Unlike [`core::slice::from_raw_parts_mut`], data pointer is allowed to be null.
    ///
    /// # Safety
    ///
    /// Check [`core::slice::from_raw_parts_mut`]
    pub const unsafe fn into_rust<'slice>(self) -> Option<&'slice mut [C]> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { core::slice::from_raw_parts_mut(self.data, self.len) })
    }
}

macro_rules! impl_slice_carrier {
    ($ty:ident) => {
        impl<C: ReprFamily> ReprFamily for $ty<C> {
            type Kind = C::Kind;
        }
        impl<C: RobustReprC> SizeFamily for $ty<C> {
            type Kind = crate::size::Sized;
        }
        impl<C: RobustReprC> NicheFamily for $ty<C> {
            type Kind = WithoutNiche;
        }

        impl<C> Borrow for $ty<C> {
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
        impl<'itm, C> ToOwned<'itm> for $ty<C> {
            #[inline(always)]
            fn to_owned(source: Self) -> Self {
                source
            }
        }

        impl<C: RobustReprC> ExternC for $ty<C> {
            type CType = Self;
        }
        impl<C: RobustReprC> EncodeOwned for $ty<C> {
            type Store = ();

            #[inline(always)]
            fn soft_encode_owned<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'d, C: RobustReprC> DecodeOwned<'d> for $ty<C> {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode_owned<'itm: 'd>(
                source: Self::CType,
                (): &mut (),
            ) -> Option<Self> {
                Some(source)
            }
        }

        impl<C: RobustReprC> Encode for $ty<C> {}
        impl<'d, C: RobustReprC> Decode<'d> for $ty<C> {}

        unsafe impl<C: RobustReprC> CheckedTransmute for $ty<C> {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<C: RobustReprC> RobustReprC for $ty<C> {}
        unsafe impl<C: RobustReprC> CFnArg for $ty<C> {}
        unsafe impl<C: RobustReprC> BorrowCast for $ty<C> {
            type AsConst = Self;
        }
        unsafe impl<C: RobustReprC> BorrowCastMut for $ty<C> {
            type AsMut = Self;
        }

        unsafe impl<C: Erase<Erased: Sized>> Erase for $ty<C> {
            type Erased = $ty<C::Erased>;
        }
    };
}

impl_slice_carrier! { CSlice }
impl_slice_carrier! { CSliceMut }
