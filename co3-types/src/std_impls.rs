#[cfg(feature = "alloc")]
use alloc::string::String;
use core::{
    cell::{Cell, UnsafeCell},
    ffi::c_void,
    marker::PhantomData,
    mem::ManuallyDrop,
    num::NonZero,
    ops::Add,
    ptr::NonNull,
};

#[cfg(feature = "alloc")]
use crate::{niche::WithCustomNiche, repr::ReprRust};
use crate::{
    niche::{NicheFamily, WithStableNiche, WithoutNiche},
    repr::{NonRobust, ReprC, ReprFamily, Robust},
    size::{MetaSized, SizeFamily, SliceLike},
};

macro_rules! non_zero_derive {
    ($($primitive:ty),+ $(,)?) => {$(
        impl ReprFamily for NonZero<$primitive> {
            type Kind = ReprC<NonRobust>;
        }
        unsafe impl SizeFamily for NonZero<$primitive> {
            type Kind = crate::size::Sized<crate::size::NonZst>;
        }
        impl NicheFamily for NonZero<$primitive> {
            type Kind = WithStableNiche;
        })+
    }
}

non_zero_derive! {
    u8, i8, u16, i16, u32, i32, u64, i64, u128, i128,
}

impl ReprFamily for c_void {
    type Kind = ReprC<Robust>;
}
unsafe impl SizeFamily for c_void {
    // TODO: Shouldn't it be ExternTypeLike?
    // I know that `mem::size_of` returns 1
    type Kind = crate::size::Sized<crate::size::NonZst>;
}
impl NicheFamily for c_void {
    type Kind = WithoutNiche;
}

impl ReprFamily for () {
    type Kind = ReprC<Robust>;
}
unsafe impl SizeFamily for () {
    type Kind = crate::size::Sized<crate::size::Zst>;
}
impl NicheFamily for () {
    type Kind = WithoutNiche;
}

impl<T: ?Sized> ReprFamily for PhantomData<T> {
    type Kind = ReprC<Robust>;
}
unsafe impl<T: ?Sized> SizeFamily for PhantomData<T> {
    type Kind = crate::size::Sized<crate::size::Zst>;
}
impl<T: ?Sized> NicheFamily for PhantomData<T> {
    type Kind = WithoutNiche;
}

impl<T: ReprFamily<Kind: Add<ReprC<NonRobust>>> + ?Sized> ReprFamily for NonNull<T> {
    type Kind = <T::Kind as Add<ReprC<NonRobust>>>::Output;
}
unsafe impl<T: ?Sized> SizeFamily for NonNull<T> {
    type Kind = crate::size::Sized<crate::size::NonZst>;
}
impl<T: ?Sized> NicheFamily for NonNull<T> {
    type Kind = WithStableNiche;
}

impl ReprFamily for str {
    type Kind = ReprC<NonRobust>;
}
unsafe impl SizeFamily for str {
    type Kind = MetaSized<SliceLike>;
}

#[cfg(feature = "alloc")]
impl ReprFamily for String {
    type Kind = ReprRust;
}
#[cfg(feature = "alloc")]
unsafe impl SizeFamily for String {
    type Kind = crate::size::Sized<crate::size::NonZst>;
}
#[cfg(feature = "alloc")]
impl NicheFamily for String {
    type Kind = WithCustomNiche;
}

impl<T: ReprFamily + ?Sized> ReprFamily for UnsafeCell<T> {
    type Kind = T::Kind;
}
unsafe impl<T: SizeFamily + ?Sized> SizeFamily for UnsafeCell<T> {
    type Kind = T::Kind;
}
impl<T: ?Sized> NicheFamily for UnsafeCell<T> {
    type Kind = WithoutNiche;
}

impl<T: ReprFamily + ?Sized> ReprFamily for Cell<T> {
    type Kind = T::Kind;
}
unsafe impl<T: SizeFamily + ?Sized> SizeFamily for Cell<T> {
    type Kind = T::Kind;
}
impl<T: ?Sized> NicheFamily for Cell<T> {
    type Kind = WithoutNiche;
}

impl<T: ReprFamily + ?Sized> ReprFamily for ManuallyDrop<T> {
    type Kind = T::Kind;
}
unsafe impl<T: SizeFamily + ?Sized> SizeFamily for ManuallyDrop<T> {
    type Kind = T::Kind;
}
impl<T: NicheFamily + ?Sized> NicheFamily for ManuallyDrop<T> {
    type Kind = T::Kind;
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::boxed::Box;

    use alloc::vec::Vec;
    use static_assertions::assert_impl_all;

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::{
        niche::WithCustomNiche,
        size::{NonZst, Sized as Co3Sized},
    };

    // TODO: Enable
    //#[test]
    //fn manually_drop_inner_without_drop() {
    //    assert_impl_all!(ManuallyDrop<u8>:
    //        ReprFamily<Kind = ReprC<Robust>>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithoutNiche>,
    //    );
    //    assert_impl_all!(&ManuallyDrop<u8>:
    //        ReprFamily<Kind = ReprC<NonRobust>>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //    );
    //    assert_impl_all!(&mut ManuallyDrop<u8>:
    //        ReprFamily<Kind = ReprC<NonRobust>>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //    );
    //    #[cfg(feature = "alloc")]
    //    assert_impl_all!(Box<ManuallyDrop<u8>>:
    //        ReprFamily<Kind = ReprC<NonRobust>>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //    );
    //    assert_impl_all!(&[ManuallyDrop<u8>]:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //    );
    //    assert_impl_all!(&mut [ManuallyDrop<u8>]:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //    );
    //    #[cfg(feature = "alloc")]
    //    assert_impl_all!(Box<[ManuallyDrop<u8>]>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //    );
    //    #[cfg(feature = "alloc")]
    //    assert_impl_all!(Vec<ManuallyDrop<u8>>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //    );
    //    assert_impl_all!([ManuallyDrop<u8>; 2]:
    //        ReprFamily<Kind = ReprC<Robust>>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithoutNiche>,
    //    );
    //    assert_impl_all!(Option<ManuallyDrop<u8>>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //    );

    //    assert_not_impl_any!(ManuallyDrop<u8>: RobustReprC);
    //}

    //#[cfg(feature = "alloc")]
    //#[test]
    //fn manually_drop_inner_with_drop() {
    //    assert_impl_all!(ManuallyDrop<String>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //    );
    //    assert_impl_all!(&ManuallyDrop<String>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //    );
    //    assert_impl_all!(&mut ManuallyDrop<String>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //    );
    //    assert_impl_all!(Box<ManuallyDrop<String>>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithStableNiche>,
    //    );
    //    assert_impl_all!(&[ManuallyDrop<String>]:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //    );
    //    assert_impl_all!(&mut [ManuallyDrop<String>]:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //    );
    //    assert_impl_all!(Box<[ManuallyDrop<String>]>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //    );
    //    assert_impl_all!(Vec<ManuallyDrop<String>>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //    );
    //    assert_impl_all!([ManuallyDrop<String>; 2]:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        NicheFamily<Kind = WithCustomNiche>,
    //    );
    //    assert_impl_all!(Option<ManuallyDrop<String>>:
    //        ReprFamily<Kind = ReprRust>,
    //        SizeFamily<Kind = Co3Sized<NonZst>>,
    //        // FIXME:
    //        //NicheFamily<Kind = WithCustomNiche>,
    //    );
    //    assert_not_impl_any!(ManuallyDrop<String>: RobustReprC);

    //    #[cfg(feature = "alloc")]
    //    assert_not_impl_any!(Box<[ManuallyDrop<String>]>: Encode, Decode<'static>);
    //    #[cfg(feature = "alloc")]
    //    assert_not_impl_any!(Vec<ManuallyDrop<String>>: Encode, Decode<'static>);
    //}

    #[test]
    fn str_is_supported() {
        assert_impl_all!(str:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = MetaSized<SliceLike>>,
        );

        assert_impl_all!(&str:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut str:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<str>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
    }

    #[test]
    fn str_decode_rejects_invalid_utf8() {
        assert_impl_all!(str: SizeFamily<Kind = MetaSized<SliceLike>>);
        assert_impl_all!(&str: SizeFamily<Kind = Co3Sized<NonZst>>);
        assert_impl_all!(&mut str: SizeFamily<Kind = Co3Sized<NonZst>>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<str>: SizeFamily<Kind = Co3Sized<NonZst>>);
    }

    #[test]
    fn robust_unsafe_cell() {
        assert_impl_all!(UnsafeCell<u8>:
            ReprFamily<Kind = ReprC<Robust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
        );
        assert_impl_all!(&UnsafeCell<u8>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&mut UnsafeCell<u8>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<u8>>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&[UnsafeCell<u8>]:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!(&mut [UnsafeCell<u8>]:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<u8>]>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<u8>>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!([UnsafeCell<u8>; 2]:
            ReprFamily<Kind = ReprC<Robust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
        );
        assert_impl_all!(Option<UnsafeCell<u8>>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
    }

    #[test]
    fn non_robust_unsafe_cell() {
        assert_impl_all!(UnsafeCell<NonZero<u8>>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
        );
        assert_impl_all!(&UnsafeCell<NonZero<u8>>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&mut UnsafeCell<NonZero<u8>>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<NonZero<u8>>>:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&[UnsafeCell<NonZero<u8>>]:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!(&mut [UnsafeCell<NonZero<u8>>]:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<NonZero<u8>>]>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<NonZero<u8>>>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!([UnsafeCell<NonZero<u8>>; 2]:
            ReprFamily<Kind = ReprC<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
        );
        assert_impl_all!(Option<UnsafeCell<NonZero<u8>>>:
            ReprFamily<Kind = ReprRust>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
    }
}
