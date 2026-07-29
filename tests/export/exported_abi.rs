use std::{mem::MaybeUninit, ptr::NonNull};

use co3::{Decode, ReprC, ffi, rust_spec::RustSpec};

trait AmbiguousX<T, const N: usize> {
    type U;

    fn ambiguous(a: &[Self::U; N]) -> Ambiguous;
}

trait AmbiguousY {
    extern "C" fn ambiguous() -> Ambiguous;
}

trait CustomExports {
    fn xor(&self, by: u8) -> Self;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[repr(u8)]
pub enum Ambiguous {
    AmbiguousX,
    AmbiguousY,
    Inherent,
    Fn,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpaqueStructU8(u8);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpaqueStructU32(u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpaqueStructU64(u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpaqueStructBool(bool);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpaqueStructI32(i32);

#[derive(Debug, Clone, Copy, PartialEq, RustSpec, ReprC)]
#[repr(transparent)]
pub enum NonOpaqueStruct<T> {
    A(T),
}

ffi! {
    #![unsafe(export("C"))]

    type OpaqueStructU8;
    type OpaqueStructU32;
    type OpaqueStructU64;
    type OpaqueStructBool;
    type OpaqueStructI32;

    #[symbol_name = "ambiguous1"]
    unsafe fn ambiguous1() -> Ambiguous;

    impl OpaqueStructU32 {
        move fn re_exported() -> Self;
    }

    impl CustomExports for OpaqueStructU8 {
        #[symbol_name = "xor_u8"]
        move fn xor(&self, by: u8) -> Self;
    }

    impl Clone for OpaqueStructBool {
        #[symbol_name = "cclone"]
        move fn clone(&self) -> Self;
    }

    impl Clone for OpaqueStructI32 {
        #[symbol_name = "lclone"]
        move fn clone(&self) -> Self;
    }

    impl Clone for self::NonOpaqueStruct<bool> {
        #[symbol_name = "nclone"]
        move fn clone(&self) -> Self;
    }

    impl Clone for NonOpaqueStruct<i32> {
        move fn clone(&self) -> Self;
    }

    impl AmbiguousX<u64, 3> for OpaqueStructU64 {
        type U = u8;
        fn ambiguous(a: &[<Self as AmbiguousX<u64, 3>>::U; 3]) -> Ambiguous;
    }

    impl AmbiguousY for OpaqueStructU64 {
        #[symbol_name = "ambiguous"]
        extern "C" fn ambiguous() -> Ambiguous;
    }

    impl OpaqueStructU32 {
        fn ambiguous() -> Ambiguous;
    }
}

ffi! {
    #![unsafe(export("Rust"))]

    impl Clone for OpaqueStructU8 {
        #[symbol_name = "clone"]
        move fn clone(&self) -> Self;
    }

    impl Clone for NonOpaqueStruct<u8> {
        move fn clone(&self) -> Self;
    }

    impl Clone for NonOpaqueStruct<i8> {
        fn clone(&self) -> Self;
    }

    impl AmbiguousX<u32, 4> for OpaqueStructU32 {
        type U = i8;
        #[symbol_name = "kita"]
        fn ambiguous(a: &[<Self as AmbiguousX<u32, 4>>::U; 4]) -> Ambiguous;
    }

    impl OpaqueStructU64 {
        #[symbol_name = "kita1"]
        unsafe extern "C" fn ambiguous() -> Ambiguous;
    }

    #[symbol_name = "kita2"]
    unsafe fn ambiguous2() -> Ambiguous;
}

impl AmbiguousX<u64, 3> for OpaqueStructU64 {
    type U = u8;

    fn ambiguous(_a: &[<Self as AmbiguousX<u64, 3>>::U; 3]) -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

impl AmbiguousX<u32, 4> for OpaqueStructU32 {
    type U = i8;

    fn ambiguous(_a: &[<Self as AmbiguousX<u32, 4>>::U; 4]) -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

impl AmbiguousY for OpaqueStructU64 {
    extern "C" fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousY
    }
}

impl CustomExports for OpaqueStructU8 {
    fn xor(&self, by: u8) -> Self {
        OpaqueStructU8(self.0 ^ by)
    }
}

impl OpaqueStructU32 {
    fn re_exported() -> Self {
        OpaqueStructU32(42)
    }
}

impl OpaqueStructU64 {
    pub const unsafe extern "C" fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

impl OpaqueStructU32 {
    pub fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

pub const unsafe fn ambiguous1() -> Ambiguous {
    Ambiguous::Fn
}

pub const unsafe extern "Rust" fn ambiguous2() -> Ambiguous {
    Ambiguous::Fn
}

#[test]
fn exported_abi() {
    let mut output = MaybeUninit::new(Ambiguous::None as _);

    unsafe extern "C" {
        fn export__OpaqueStructU32__ambiguous() -> u8;

        fn ambiguous() -> u8;
        fn ambiguous1() -> u8;

        #[symbol_name = "cclone"]
        fn export_opaque_clone_bool(
            handle_ptr: *const OpaqueStructBool,
        ) -> *mut OpaqueStructBool;
        #[symbol_name = "nclone"]
        fn export_non_opaque_clone_bool(handle_ptr: *const u8) -> u8;
        #[symbol_name = "xor_u8"]
        fn export_opaque_xor_u8(
            handle_ptr: *const OpaqueStructU8,
            by: u8,
        ) -> *mut OpaqueStructU8;

        fn export__AmbiguousX_u64_3__OpaqueStructU64__ambiguous(
            a: &[u8; 3],
        ) -> u8;

        #[symbol_name = "export__OpaqueStructU32__re_exported"]
        fn re_exported() -> *mut OpaqueStructU32;
    }

    unsafe extern "Rust" {
        fn kita(a: *const [i8; 4]) -> u8;
        fn kita1() -> u8;
        fn kita2() -> u8;

        #[symbol_name = "export__Clone__NonOpaqueStruct_u8__clone"]
        fn export_non_opaque_clone_u8(handle: *const u8) -> u8;
        #[symbol_name = "clone"]
        fn export_opaque_clone_u8(
            handle: *const NonNull<Extern>,
        ) -> *mut OpaqueStructU8;
    }

    unsafe {
            export__OpaqueStructU32__ambiguous(output.as_mut_ptr())
        ;;
        let inherent: Ambiguous = Decode::decode(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::Inherent, inherent);

            export__AmbiguousX_u64_3__OpaqueStructU64__ambiguous(&[12; 3], output.as_mut_ptr())
        ;;
        let ambiguous_x: Ambiguous = Decode::decode(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousX, ambiguous_x);

            kita((&[13_i8; 4]).encode(&mut ()), output.as_mut_ptr())
        ;;
        let ambiguous_x: Ambiguous = Decode::decode(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousX, ambiguous_x);

        let inherent = co3::decode(unsafe { kita1() }).unwrap();
        assert_eq!(Ambiguous::Inherent, inherent);

        let custom_fn = co3::decode(unsafe { kita2() }).unwrap();
        assert_eq!(Ambiguous::Fn, custom_fn);

        let custom_fn = co3::decode(unsafe { ambiguous1() }).unwrap();
        assert_eq!(Ambiguous::Fn, custom_fn);

        let custom_fn = co3::decode(unsafe { ambiguous() }).unwrap();
        assert_eq!(Ambiguous::AmbiguousY, custom_fn);
    }

    unsafe {
        let mut output = MaybeUninit::new(NonNull::dangling());

        let custom_fn = Box::from_raw(re_exported());
        assert_eq!(OpaqueStructU32(42_u32), *custom_fn);
    }

    unsafe {
        let opaque_bool = OpaqueStructBool(true);
        let opaque_u8 = OpaqueStructU8(11_u8);

        let opaque_bool_ptr = &opaque_bool as *const OpaqueStructBool;
        let opaque_u8_ptr = &opaque_u8 as *const OpaqueStructU8;

        let mut opaque_bool_clone_out = MaybeUninit::new(NonNull::dangling());

            export_opaque_clone_bool(opaque_bool_ptr.cast(), opaque_bool_clone_out.as_mut_ptr())
        ;;
        let mut opaque_u8_clone_out = MaybeUninit::new(NonNull::dangling());
            export_opaque_clone_u8(opaque_u8_ptr.cast(), opaque_u8_clone_out.as_mut_ptr(),)
        ;;
        let opaque_u8_clone = Box::from_raw(opaque_u8_clone_out.assume_init().as_ptr().cast());
        assert_eq!(OpaqueStructU8(11_u8), *opaque_u8_clone);
        let mut opaque_xor_out = MaybeUninit::new(NonNull::dangling());
            export_opaque_xor_u8(opaque_u8_ptr.cast(), 7, opaque_xor_out.as_mut_ptr())
        ;;
        let opaque_xor = Box::from_raw(opaque_xor_out.assume_init().as_ptr().cast());
        assert_eq!(OpaqueStructU8(12_u8), *opaque_xor);
        let opaque_bool_clone = Box::from_raw(opaque_bool_clone_out.assume_init().as_ptr().cast());

        assert_eq!(OpaqueStructBool(true), *opaque_bool_clone);
    }

    unsafe {
        let non_opaque_bool = NonOpaqueStruct::A(true);
        let non_opaque_u8 = NonOpaqueStruct::A(11_u8);

        let mut non_opaque_bool_clone_out = MaybeUninit::new(171);

            export_non_opaque_clone_bool(
                (&non_opaque_bool).encode(&mut ()),
                non_opaque_bool_clone_out.as_mut_ptr(),
            )
        ;;
        let mut non_opaque_u8_clone_out = MaybeUninit::new(171);
            export_non_opaque_clone_u8(
                (&non_opaque_u8).encode(&mut ()),
                non_opaque_u8_clone_out.as_mut_ptr(),
            )
        ;;
        let non_opaque_u8_clone =
            <NonOpaqueStruct<u8> as Decode>::decode(non_opaque_u8_clone_out.assume_init()).unwrap();
        assert_eq!(NonOpaqueStruct::A(11_u8), non_opaque_u8_clone);

        let non_opaque_bool_clone =
            <NonOpaqueStruct<bool> as Decode>::decode(non_opaque_bool_clone_out.assume_init())
                .unwrap();

        assert_eq!(NonOpaqueStruct::A(true), non_opaque_bool_clone);
    }
}

