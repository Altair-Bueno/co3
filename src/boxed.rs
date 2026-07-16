//! Logic related to the conversion of boxed values to and from FFI-compatible representation.

use alloc_crate::boxed::Box;
use core::ptr::NonNull;

use crate::{
    CFnArg, Decode, Encode, ExternC, RobustReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, ToOwned},
    ir::ReprFamily,
    niche::{NicheFamily, WithoutNiche},
    size::{SizeFamily, Spread},
    slice::{CSlice, CSliceMut},
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
};

/// Owned pointer `Box<C>`.
///
/// If the data pointer is set to `null`, the struct represents `Option<Box<C>>`.
#[repr(C)]
pub struct CBox<C> {
    pub(crate) data: *mut C,
}

/// Owned slice `Box<[C]>` with a defined C ABI layout. Consists of a data pointer and a length.
/// Used in place of a function out-pointer to transfer ownership of the slice to the caller.
/// If the data pointer is set to `null`, the struct represents `Option<Box<[C]>>`.
#[repr(C)]
pub struct CBoxedSlice<C> {
    pub(crate) data: *mut C,
    len: usize,
}

impl<C> core::fmt::Debug for CBox<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct(stringify!(CBox))
            .field("data", &self.data)
            .finish_non_exhaustive()
    }
}

impl<C> core::fmt::Debug for CBoxedSlice<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.data.is_null() {
            f.debug_struct(stringify!(CBoxedSlice))
                .field("data", &self.data)
                .finish_non_exhaustive()
        } else {
            f.debug_struct(stringify!(CBoxedSlice))
                .field("data", &self.data)
                .field("len", &self.len)
                .finish()
        }
    }
}

impl<C> PartialEq for CBox<C> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

impl<C> PartialEq for CBoxedSlice<C> {
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

impl<C> Eq for CBox<C> {}
impl<C> Eq for CBoxedSlice<C> {}

impl<C> PartialOrd for CBox<C> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<C> PartialOrd for CBoxedSlice<C> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<C> Ord for CBox<C> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.data.cmp(&other.data)
    }
}
impl<C> Ord for CBoxedSlice<C> {
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

impl<C> Clone for CBox<C> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C> Clone for CBoxedSlice<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for CBox<C> {}
impl<C> Copy for CBoxedSlice<C> {}

impl<C> CBox<C> {
    /// Create [`Self`] from a [`Box<C>`].
    pub fn from_box(source: Box<C>) -> Self {
        Self {
            data: Box::into_raw(source),
        }
    }

    /// Create [`Self`] from a raw data pointer
    pub(crate) const fn from_raw_parts(data: NonNull<C>) -> Self {
        Self {
            data: data.as_ptr(),
        }
    }
}

impl<C> CBox<C> {
    /// Set the pointer to null.
    pub(crate) const NICHE_VALUE: Self = Self {
        data: core::ptr::null_mut(),
    };

    pub(crate) unsafe fn read(self) -> C {
        unsafe { self.data.read() }
    }

    /// Returns `true` if the option is a `None` value.
    pub(crate) const fn is_niche(&self) -> bool {
        self.data.is_null()
    }
}

impl<C> CBox<C> {
    /// Convert [`Self`] into [`Box<C>`]. Returns `None` if pointer is null.
    ///
    /// # Safety
    ///
    /// Check [`Box::from_raw`].
    unsafe fn into_rust(self) -> Option<Box<C>> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { Box::from_raw(self.data) })
    }
}

impl<C> CBoxedSlice<C> {
    /// Create [`Self`] from a [`Box<[T]>`]
    pub fn from_boxed_slice(source: Box<[C]>) -> Self {
        let mut boxed_slice = core::mem::ManuallyDrop::new(source);

        Self {
            data: boxed_slice.as_mut_ptr(),
            len: boxed_slice.len(),
        }
    }

    /// Create [`Self`] from a raw data pointer and slice metadata.
    pub(crate) const fn from_raw_parts(data: NonNull<C>, len: usize) -> Self {
        Self {
            data: data.as_ptr(),
            len,
        }
    }

    /// Convert [`Self`] into [`Box<[C]>`]. Returns `None` if pointer is null.
    ///
    /// # Safety
    ///
    /// Check [`Box::from_raw`].
    pub(crate) unsafe fn into_rust(self) -> Option<Box<[C]>> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(self.data, self.len)) })
    }
}

impl<C> CBoxedSlice<C> {
    /// Set the slice's data pointer to null
    pub(crate) const NICHE_VALUE: Self = Self {
        data: core::ptr::null_mut(),
        len: 0,
    };

    pub(crate) const fn is_niche(&self) -> bool {
        self.data.is_null()
    }

    pub(crate) const fn data(&self) -> *mut C {
        self.data
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
    }
}

macro_rules! impl_boxed_carrier {
    ($ty:ident) => {
        impl<C: ReprFamily> ReprFamily for $ty<C> {
            type Kind = C::Kind;
        }
        unsafe impl<C> SizeFamily for $ty<C> {
            type Kind = crate::size::Sized<crate::size::NonZst>;
        }
        impl<C> NicheFamily for $ty<C> {
            type Kind = WithoutNiche;
        }

        unsafe impl<C> Borrow for $ty<C> {
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
        unsafe impl<C: RobustReprC> EncodeOwned for $ty<C> {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl<'d, C: RobustReprC> DecodeOwned<'d> for $ty<C> {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
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
    };
}

impl_boxed_carrier! { CBox }
impl_boxed_carrier! { CBoxedSlice }

unsafe impl<C: RobustReprC> BorrowCast for CBox<C> {
    type AsConst = *const C;
}
unsafe impl<C: RobustReprC> BorrowCastMut for CBox<C> {
    type AsMut = *mut C;
}

unsafe impl<C: RobustReprC> BorrowCast for CBoxedSlice<C> {
    type AsConst = CSlice<C>;
}
unsafe impl<C: RobustReprC> BorrowCastMut for CBoxedSlice<C> {
    type AsMut = CSliceMut<C>;
}

impl<C: RobustReprC> Spread for CBoxedSlice<C> {
    type Part1 = *mut C;
    type Part2 = usize;

    #[inline(always)]
    fn into_parts(self) -> (Self::Part1, Self::Part2) {
        (self.data, self.len)
    }

    #[inline(always)]
    fn from_parts(data: Self::Part1, len: Self::Part2) -> Self {
        Self { data, len }
    }
}
