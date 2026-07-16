//! Logic related to the conversion of boxed values to and from FFI-compatible representation.

use alloc_crate::boxed::Box;
use core::ptr::NonNull;

use crate::{
    CFnArg, Decode, Encode, ExternC, RobustReprC,
    alloc::{Allocator, Global},
    borrow::{Borrow, BorrowCast, BorrowCastMut, ToOwned},
    ir::ReprFamily,
    niche::{NicheFamily, WithoutNiche},
    size::{SizeFamily, Spread},
    slice::{CSlice, CSliceMut},
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
};

/// Owned pointer `Box<C>` with a deallocate function.
///
/// If the data pointer is set to `null`, the struct represents `Option<Box<C>>`.
#[repr(C)]
pub struct CBox<C, A: Allocator = Global> {
    pub(crate) data: *mut C,
    allocator: A,
}

/// Owned slice `Box<[C]>` with a defined C ABI layout. Consists of a data pointer and a length.
/// Used in place of a function out-pointer to transfer ownership of the slice to the caller.
/// If the data pointer is set to `null`, the struct represents `Option<Box<[C]>>`.
#[repr(C)]
pub struct CBoxedSlice<C, A: Allocator = Global> {
    pub(crate) data: *mut C,
    len: usize,
    allocator: A,
}

impl<C, A: Allocator> core::fmt::Debug for CBox<C, A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct(stringify!(CBox))
            .field("data", &self.data)
            .finish_non_exhaustive()
    }
}

impl<C, A: Allocator> core::fmt::Debug for CBoxedSlice<C, A> {
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

impl<C, A: Allocator> PartialEq for CBox<C, A> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

impl<C, A: Allocator> PartialEq for CBoxedSlice<C, A> {
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

impl<C, A: Allocator> Eq for CBox<C, A> {}
impl<C, A: Allocator> Eq for CBoxedSlice<C, A> {}

impl<C, A: Allocator> PartialOrd for CBox<C, A> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<C, A: Allocator> PartialOrd for CBoxedSlice<C, A> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<C, A: Allocator> Ord for CBox<C, A> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.data.cmp(&other.data)
    }
}
impl<C, A: Allocator> Ord for CBoxedSlice<C, A> {
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

impl<C, A: Allocator> Clone for CBox<C, A> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C, A: Allocator> Clone for CBoxedSlice<C, A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C, A: Allocator> Copy for CBox<C, A> {}
impl<C, A: Allocator> Copy for CBoxedSlice<C, A> {}

impl<C> CBox<C> {
    /// Create [`Self`] from a [`Box<C>`].
    pub fn from_box(source: Box<C>) -> Self {
        Self {
            data: Box::into_raw(source),
            allocator: Global,
        }
    }

    /// Create [`Self`] from a raw data pointer
    pub(crate) const fn from_raw_parts(data: NonNull<C>) -> Self {
        Self {
            data: data.as_ptr(),
            allocator: Global,
        }
    }
}

impl<C, A: Allocator> CBox<C, A> {
    /// Set the pointer to null.
    pub(crate) const NICHE_VALUE: Self = Self {
        data: core::ptr::null_mut(),
        // TODO: Figure out if this is correct
        // SAFETY: allocator will never be used
        allocator: unsafe { core::mem::zeroed() },
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
            allocator: Global,
        }
    }

    /// Create [`Self`] from a raw data pointer and slice metadata.
    pub(crate) const fn from_raw_parts(data: NonNull<C>, len: usize) -> Self {
        Self {
            data: data.as_ptr(),
            len,
            allocator: Global,
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

impl<C, A: Allocator> CBoxedSlice<C, A> {
    /// Set the slice's data pointer to null
    pub(crate) const NICHE_VALUE: Self = Self {
        data: core::ptr::null_mut(),
        len: 0,
        // SAFETY: allocator will never be used
        allocator: unsafe { core::mem::zeroed() },
    };

    pub(crate) unsafe fn deallocate(&self) -> bool {
        if self.data.is_null() || self.len == 0 {
            return true;
        }

        if let Ok(layout) = core::alloc::Layout::array::<C>(self.len) {
            unsafe {
                self.allocator
                    .deallocate(NonNull::new_unchecked(self.data.cast()), layout);
            }

            return true;
        }

        false
    }

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
        impl<C: ReprFamily, A: Allocator> ReprFamily for $ty<C, A> {
            type Kind = C::Kind;
        }
        unsafe impl<C, A: Allocator> SizeFamily for $ty<C, A> {
            type Kind = crate::size::Sized<crate::size::NonZst>;
        }
        impl<C, A: Allocator> NicheFamily for $ty<C, A> {
            type Kind = WithoutNiche;
        }

        unsafe impl<C, A: Allocator> Borrow for $ty<C, A> {
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
        impl<'itm, C, A: Allocator> ToOwned<'itm> for $ty<C, A> {
            #[inline(always)]
            fn to_owned(source: Self) -> Self {
                source
            }
        }

        impl<C: RobustReprC, A: Allocator> ExternC for $ty<C, A> {
            type CType = Self;
        }
        unsafe impl<C: RobustReprC, A: Allocator> EncodeOwned for $ty<C, A> {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl<'d, C: RobustReprC, A: Allocator> DecodeOwned<'d> for $ty<C, A> {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl<C: RobustReprC, A: Allocator> Encode for $ty<C, A> {}
        impl<'d, C: RobustReprC, A: Allocator> Decode<'d> for $ty<C, A> {}

        unsafe impl<C: RobustReprC, A: Allocator> CheckedTransmute for $ty<C, A> {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<C: RobustReprC, A: Allocator> RobustReprC for $ty<C, A> {}
        unsafe impl<C: RobustReprC, A: Allocator> CFnArg for $ty<C, A> {}
    };
}

impl_boxed_carrier! { CBox }
impl_boxed_carrier! { CBoxedSlice }

unsafe impl<C: RobustReprC, A: Allocator> BorrowCast for CBox<C, A> {
    type AsConst = *const C;
}
unsafe impl<C: RobustReprC, A: Allocator> BorrowCastMut for CBox<C, A> {
    type AsMut = *mut C;
}

unsafe impl<C: RobustReprC, A: Allocator> BorrowCast for CBoxedSlice<C, A> {
    type AsConst = CSlice<C>;
}
unsafe impl<C: RobustReprC, A: Allocator> BorrowCastMut for CBoxedSlice<C, A> {
    type AsMut = CSliceMut<C>;
}

impl<C: RobustReprC, A: Allocator> Spread for CBoxedSlice<C, A> {
    type Part1 = *mut C;
    type Part2 = usize;

    #[inline(always)]
    fn into_parts(self) -> (Self::Part1, Self::Part2) {
        (self.data, self.len)
    }

    #[inline(always)]
    fn from_parts(data: Self::Part1, len: Self::Part2) -> Self {
        Self {
            data,
            len,
            // SAFETY: this matches the existing niche construction. The allocator is only
            // meaningful for carriers that came from co3-owned allocations.
            allocator: unsafe { core::mem::zeroed() },
        }
    }
}
