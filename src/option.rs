//! FFI-safe equivalent of [`core::option`] related functionality

use crate::{
    CFnArg, ExternC, FfiReturn, ReprC,
    borrow::{Borrow, BorrowCast, ToOwned},
    handle::Erase,
    ir::ReprFamily,
    niche::{Niche, NicheFamily, WithoutNiche},
    size::SizeFamily,
    stored::{SoftDecodeOwned, SoftEncodeOwned},
    transmute::CheckedTransmute,
};

/// FFI-safe equivalent of [`core::option::Option`] for [`crate::ReprC`] types
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub struct COption<T> {
    tag: u8,
    payload: T,
}

impl<T> COption<T> {
    /// Construct no value
    #[expect(non_snake_case)]
    pub const fn None() -> Self {
        Self {
            tag: 0,
            // SAFETY: `ReprC` types can't have any trap representations here
            payload: unsafe { core::mem::zeroed() },
        }
    }

    /// Construct some value
    #[expect(non_snake_case)]
    pub const fn Some(value: T) -> Self {
        Self {
            tag: 1,
            payload: value,
        }
    }

    pub(crate) const fn none() -> Self {
        Self {
            tag: 2,
            payload: unsafe { core::mem::zeroed() },
        }
    }
}

impl<T> From<Option<T>> for COption<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Some(value),
            None => Self::None(),
        }
    }
}

impl<T> TryFrom<COption<T>> for Option<T> {
    type Error = FfiReturn;

    fn try_from(value: COption<T>) -> Result<Self, Self::Error> {
        match value.tag {
            0 => Ok(None),
            1 => Ok(Some(value.payload)),
            _ => Err(FfiReturn::TrapRepresentation),
        }
    }
}

impl<T: Copy> Copy for COption<T> {}
impl<T: Copy> Clone for COption<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ReprFamily> ReprFamily for COption<T> {
    type Kind = T::Kind;
}
impl<T> SizeFamily for COption<T> {
    type Kind = crate::size::Sized;
}
impl<T> NicheFamily for COption<T> {
    type Kind = WithoutNiche;
}

unsafe impl<T: ReprC + CheckedTransmute> CheckedTransmute for COption<T> {
    #[inline(always)]
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

unsafe impl<T: ReprC> ReprC for COption<T> {}
unsafe impl<T: ReprC + Copy> CFnArg for COption<T> {}

impl<T: ReprC> ExternC for COption<T> {
    type CType = COption<T>;
}
impl<R: ReprC + Copy> Niche for Option<R>
where
    Self: ExternC<CType = COption<R>>,
{
    const NICHE_VALUE: Self::CType = COption::none();
}
impl<T: ReprC + Copy> SoftEncodeOwned for COption<T> {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self
    }
}
impl<'d, T: ReprC + Copy> SoftDecodeOwned<'d> for COption<T> {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}
impl<T> Borrow for COption<T> {
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

impl<'itm, T> ToOwned<'itm> for COption<T> {
    #[inline(always)]
    fn to_owned(source: Self) -> Self {
        source
    }
}

unsafe impl<T: Erase<Erased: Sized>> Erase for COption<T> {
    type Erased = COption<T::Erased>;
}

unsafe impl<T: BorrowCast> BorrowCast for COption<T> {
    type AsConst = COption<T::AsConst>;
    type AsMut = COption<T::AsMut>;
}
