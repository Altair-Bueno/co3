#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;
extern crate self as co3_types;

#[cfg(feature = "derive")]
pub use co3_types_derive::TypeFamily;

pub mod mutability;
pub mod niche;
mod primitives;
pub mod repr;
pub mod size;
mod std_impls;
mod tuple;

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    #[cfg(feature = "alloc")]
    use alloc::vec::Vec;

    use static_assertions::assert_impl_all;

    use super::*;
    use crate::co3_types::{
        niche::{NicheFamily, WithCustomNiche, WithStableNiche, WithoutNiche},
        repr::{NonRobust, ReprFamily, Stable, Unstable},
        size::{NonZst, SizeFamily, Sized as Co3Sized},
    };

    #[test]
    fn transparent_type() {
        assert_impl_all!(bool:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!(&bool:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&mut bool:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<bool>:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&[bool]:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [bool]:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[bool]>:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<bool>:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!([bool; 2]:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        // FIXME:
        //assert_impl_all!(Option<bool>:
        //    ReprFamily<Kind = Unstable>,
        //    SizeFamily<Kind = Co3Sized<NonZst>>,
        //    NicheFamily<Kind = WithCustomNiche>,
        //);
    }

    #[test]
    fn robust_ref() {
        assert_impl_all!(&&u8:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&mut &u8:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&[&u8]:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!(&mut [&u8]:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&u8]>:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&u8>:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!([&u8; 2]:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!(Option<&u8>:
            // FIXME:
            //ReprFamily<Kind = Stable<Robust>>,
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
        );
    }

    #[test]
    fn transparent_ref() {
        assert_impl_all!(&&bool:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&mut &bool:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&[&bool]:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [&bool]:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&bool]>:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&bool>:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!([&bool; 2]:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!(Option<&bool>:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
        );
    }

    #[test]
    fn robust_ref_mut() {
        assert_impl_all!(&&mut u8:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&mut &mut u8:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut u8>:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&[&mut u8]:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!(&mut [&mut u8]:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut u8]>:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut u8>:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!([&mut u8; 2]:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!(Option<&mut u8>:
            // FIXME:
            //ReprFamily<Kind = Stable<Robust>>,
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
        );
    }

    #[test]
    fn transparent_ref_mut() {
        assert_impl_all!(&&mut bool:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&mut &mut bool:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut bool>:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithStableNiche>,
        );
        assert_impl_all!(&[&mut bool]:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!(&mut [&mut bool]:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut bool]>:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut bool>:
            ReprFamily<Kind = Unstable>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!([&mut bool; 2]:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithCustomNiche>,
        );
        assert_impl_all!(Option<&mut bool>:
            ReprFamily<Kind = Stable<NonRobust>>,
            SizeFamily<Kind = Co3Sized<NonZst>>,
            NicheFamily<Kind = WithoutNiche>,
        );
    }
}
