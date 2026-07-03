//! FFI-safe equivalent of [`core::option`] related functionality

use core::mem::MaybeUninit;

use crate::{
    CFnArg, Decode, Encode, ExternC, FfiReturn, RobustReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, ToOwned},
    ir::ReprFamily,
    niche::{NicheFamily, WithoutNiche},
    size::SizeFamily,
    stored::{DecodeOwned, EmptyStore, EncodeOwned},
    transmute::CheckedTransmute,
};

/// FFI-safe equivalent of [`core::option::Option`] for [`crate::RobustReprC`] types
#[repr(C)]
pub struct ReprCOption<T> {
    tag: u8,
    payload: MaybeUninit<T>,
}

impl<T: core::fmt::Debug> core::fmt::Debug for ReprCOption<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.tag {
            0 => f.write_str("ReprCOption::None"),
            1 => f
                .debug_tuple("ReprCOption::Some")
                .field(unsafe { self.payload.assume_init_ref() })
                .finish(),
            tag => f
                .debug_struct("ReprCOption::<invalid>")
                .field("tag", &tag)
                .finish(),
        }
    }
}

impl<T: PartialEq> PartialEq for ReprCOption<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self.tag, other.tag) {
            (0, 0) => true,
            (1, 1) => {
                let self_payload = unsafe { self.payload.assume_init_ref() };
                let other_payload = unsafe { other.payload.assume_init_ref() };

                self_payload == other_payload
            }
            _ => false,
        }
    }
}
impl<T: PartialOrd> PartialOrd for ReprCOption<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        match (self.tag, other.tag) {
            (0, 0) => Some(core::cmp::Ordering::Equal),
            (0, 1) => Some(core::cmp::Ordering::Less),
            (1, 0) => Some(core::cmp::Ordering::Greater),
            (1, 1) => {
                let self_payload = unsafe { self.payload.assume_init_ref() };
                let other_payload = unsafe { other.payload.assume_init_ref() };

                self_payload.partial_cmp(other_payload)
            }
            _ => None,
        }
    }
}

impl<T> ReprCOption<T> {
    pub(crate) const NICHE_VALUE: Self = Self {
        tag: 2,
        payload: MaybeUninit::uninit(),
    };

    /// Construct no value
    #[expect(non_snake_case)]
    pub const fn None() -> Self {
        Self {
            tag: 0,
            payload: MaybeUninit::uninit(),
        }
    }

    /// Construct some value
    #[expect(non_snake_case)]
    pub const fn Some(value: T) -> Self {
        Self {
            tag: 1,
            payload: MaybeUninit::new(value),
        }
    }

    fn forward_payload<U>(self) -> ReprCOption<U> {
        let mut output = MaybeUninit::<ReprCOption<U>>::uninit();

        unsafe {
            let output_ptr = output.as_mut_ptr();
            core::ptr::addr_of_mut!((*output_ptr).tag).write(self.tag);

            core::ptr::copy_nonoverlapping(
                self.payload.as_ptr().cast::<u8>(),
                core::ptr::addr_of_mut!((*output_ptr).payload).cast::<u8>(),
                core::cmp::min(size_of::<T>(), size_of::<U>()),
            );

            output.assume_init()
        }
    }
}

impl<T: Copy> Copy for ReprCOption<T> {}
impl<T: Copy> Clone for ReprCOption<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> From<Option<T>> for ReprCOption<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Some(value),
            None => Self::None(),
        }
    }
}

impl<T> TryFrom<ReprCOption<T>> for Option<T> {
    type Error = FfiReturn;

    fn try_from(value: ReprCOption<T>) -> Result<Self, Self::Error> {
        match value.tag {
            0 => Ok(None),
            1 => Ok(Some(unsafe { value.payload.assume_init() })),
            _ => Err(FfiReturn::TrapRepresentation),
        }
    }
}

impl<T: ReprFamily> ReprFamily for ReprCOption<T> {
    type Kind = T::Kind;
}
impl<T> SizeFamily for ReprCOption<T> {
    type Kind = crate::size::Sized;
}
impl<T> NicheFamily for ReprCOption<T> {
    type Kind = WithoutNiche;
}

impl<T: Borrow> Borrow for ReprCOption<T> {
    type Borrowed<'itm>
        = ReprCOption<T::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        match self.tag {
            1 => {
                let payload = unsafe { self.payload.assume_init() };
                ReprCOption::Some(payload.borrow(owner))
            }
            _ => self.forward_payload(),
        }
    }
}
impl<'itm, T: ToOwned<'itm>> ToOwned<'itm> for ReprCOption<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        match source.tag {
            1 => {
                let payload = unsafe { source.payload.assume_init() };
                Self::Some(T::to_owned(payload))
            }
            _ => source.forward_payload(),
        }
    }
}

impl<T: ExternC<CType: Sized>> ExternC for ReprCOption<T> {
    type CType = ReprCOption<T::CType>;
}
unsafe impl<T: EncodeOwned<CType: Copy>> EncodeOwned for ReprCOption<T> {
    type Store = T::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        match self.tag {
            1 => {
                let payload = unsafe { self.payload.assume_init() };
                ReprCOption::Some(payload.soft_encode(store))
            }
            _ => self.forward_payload(),
        }
    }
}
unsafe impl<'d, T: DecodeOwned<'d, CType: Copy>> DecodeOwned<'d> for ReprCOption<T> {
    type Store = T::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        match source.tag {
            1 => {
                let payload = unsafe { T::soft_decode(source.payload.assume_init(), store)? };
                Some(Self::Some(payload))
            }
            _ => Some(source.forward_payload()),
        }
    }
}

impl<T: Encode<CType: Copy>> Encode for ReprCOption<T> {}
impl<'d, T: Decode<'d, CType: Copy>> Decode<'d> for ReprCOption<T> {}

unsafe impl<T: CheckedTransmute<CType: Copy>> CheckedTransmute for ReprCOption<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        match target.tag {
            1 => unsafe { T::is_valid(&*target.payload.as_ptr()) },
            _ => true,
        }
    }
}

unsafe impl<T: RobustReprC> RobustReprC for ReprCOption<T> {}
unsafe impl<T: RobustReprC + Copy> CFnArg for ReprCOption<T> {}

unsafe impl<T: BorrowCast<AsConst: Copy> + Copy> BorrowCast for ReprCOption<T> {
    type AsConst = ReprCOption<T::AsConst>;
}
unsafe impl<T: BorrowCastMut<AsMut: Copy> + Copy> BorrowCastMut for ReprCOption<T> {
    type AsMut = ReprCOption<T::AsMut>;
}

unsafe impl<T: EmptyStore> EmptyStore for Option<T> {}
