#[cfg(feature = "alloc")]
use alloc_crate::{borrow::ToOwned as StdToOwned, boxed::Box, vec::Vec};

#[cfg(feature = "alloc")]
use crate::size::{MetaSized, SizeFamily};
use crate::{RobustReprC, stored::ArrayStore};

// TODO: Remove this once extern types are stable
// https://github.com/rust-lang/rust/issues/43467
#[cfg(feature = "alloc")]
trait NonExternTypeLike {}
#[cfg(feature = "alloc")]
impl<K> NonExternTypeLike for MetaSized<K> {}
#[cfg(feature = "alloc")]
impl NonExternTypeLike for crate::size::Sized {}

/// A layout-compatible borrowed view of a robust C representation.
///
/// # Safety
///
/// - only owned to borrowed const pointer casting is allowed
// TODO: Stupid trait with a stupid name
pub unsafe trait BorrowCast: RobustReprC {
    type AsConst: RobustReprC + ?Sized;
}

/// A layout-compatible mutably borrowed view of a robust C representation.
///
/// # Safety
///
/// - only owned to borrowed mut pointer casting is allowed
pub unsafe trait BorrowCastMut: RobustReprC {
    type AsMut: RobustReprC + ?Sized;
}

#[inline(always)]
pub fn borrow_cast<C: BorrowCast<AsConst: Copy> + Copy>(source: C) -> C::AsConst {
    unsafe { core::mem::transmute_copy(&source) }
}

#[inline(always)]
pub fn borrow_cast_mut<C: BorrowCastMut<AsMut: Copy> + Copy>(source: C) -> C::AsMut {
    unsafe { core::mem::transmute_copy(&source) }
}

/// A trait for structurally borrowing data.
///
/// It should hold that `<T::CType as BorrowCast>::AsConst == <T::Borrowed as ExternC>::CType`
pub trait Borrow: Sized {
    type Borrowed<'itm>
    where
        Self: 'itm;

    type Owner: Default;
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm;
}
// TODO: Join the 2 traits?
pub trait ToOwned<'itm>: Borrow {
    fn to_owned(source: Self::Borrowed<'itm>) -> Self;
}

impl<R: Borrow> Borrow for Option<R> {
    type Borrowed<'itm>
        = Option<R::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = R::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self.map(|value| value.borrow(store))
    }
}
impl<'itm, R: ToOwned<'itm>> ToOwned<'itm> for Option<R> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source.map(R::to_owned)
    }
}

impl<T: Borrow, E: Borrow> Borrow for Result<T, E> {
    type Borrowed<'itm>
        = Result<T::Borrowed<'itm>, E::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = Option<Result<T::Owner, E::Owner>>;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        match self {
            Ok(value) => {
                let ok_store = store.insert(Ok(Default::default())).as_mut();
                Ok(value.borrow(unsafe { ok_store.unwrap_unchecked() }))
            }
            Err(err) => {
                let err_store = store.insert(Err(Default::default())).as_mut();
                Err(err.borrow(unsafe { err_store.unwrap_err_unchecked() }))
            }
        }
    }
}
impl<'itm, T: ToOwned<'itm>, E: ToOwned<'itm>> ToOwned<'itm> for Result<T, E> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        match source {
            Ok(value) => Ok(T::to_owned(value)),
            Err(err) => Err(E::to_owned(err)),
        }
    }
}

impl<R: ?Sized> Borrow for &R {
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
impl<'itm, 'a: 'itm, R: ?Sized> ToOwned<'itm> for &'a R {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

impl<R: ?Sized> Borrow for &mut R {
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
impl<'itm, 'a: 'itm, R: ?Sized> ToOwned<'itm> for &'a mut R {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

#[cfg(feature = "alloc")]
// NOTE: extern types cannot be borrowed, only moved
impl<R: SizeFamily<Kind: NonExternTypeLike> + ?Sized> Borrow for Box<R> {
    type Borrowed<'itm>
        = &'itm R
    where
        Self: 'itm;

    // NOTE: If just `Self` was used, a potentially
    // large value would be placed on the stack
    type Owner = Option<Self>;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        store.insert(self)
    }
}
#[cfg(feature = "alloc")]
impl<'itm, R: SizeFamily<Kind: NonExternTypeLike> + StdToOwned + ?Sized> ToOwned<'itm> for Box<R>
where
    R::Owned: Into<Self>,
{
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        StdToOwned::to_owned(source).into()
    }
}

#[cfg(feature = "alloc")]
impl<R> Borrow for Vec<R> {
    type Borrowed<'itm>
        = &'itm [R]
    where
        Self: 'itm;

    type Owner = Self;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        *store = self;
        store
    }
}
#[cfg(feature = "alloc")]
impl<'itm, R: Clone> ToOwned<'itm> for Vec<R> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source.to_vec()
    }
}

impl<R: Borrow, const N: usize> Borrow for [R; N] {
    type Borrowed<'itm>
        = [R::Borrowed<'itm>; N]
    where
        Self: 'itm;

    type Owner = ArrayStore<R::Owner, N>;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        let mut borrowed: [_; N] = [const { core::mem::MaybeUninit::uninit() }; N];

        for ((elem, substore), borrow) in self.into_iter().zip(&mut store.0).zip(&mut borrowed) {
            borrow.write(elem.borrow(substore));
        }

        // TODO: use https://github.com/rust-lang/rust/issues/96097
        unsafe {
            core::mem::transmute_copy::<
                [core::mem::MaybeUninit<R::Borrowed<'itm>>; N],
                [R::Borrowed<'itm>; N],
            >(&borrowed)
        }
    }
}
impl<'itm, R: ToOwned<'itm>, const N: usize> ToOwned<'itm> for [R; N] {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source.map(R::to_owned)
    }
}
