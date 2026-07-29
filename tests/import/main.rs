mod imported_abi;
//mod opaque;

//#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//#[repr(C)]
//pub struct Robust(u64);
//
//#[extern_type]
//#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//#[repr(transparent)]
//pub struct Transparent((u32, u32));
//
//#[ffi]
//impl Robust {
//    pub fn take_ref(&self) -> &Self {
//        unreachable!("replaced by ffi")
//    }
//}
//
//#[ffi]
//pub fn freestanding_returns_non_local(input: &u32) -> &u32 {
//    unreachable!("replaced by ffi")
//}
//
//#[ffi]
//pub fn freestanding_returns_local_ref(input: &(u32, u32)) -> (u32, u32) {
//    unreachable!("replaced by ffi")
//}
//
//#[ffi]
//pub fn freestanding_returns_local_slice(input: &[(u32, u32)]) -> Box<[(u32, u32)]> {
//    unreachable!("replaced by ffi")
//}
//
//#[ffi]
//pub fn freestanding_returns_boxed_slice(input: Box<[u32]>) -> Box<[u32]> {
//    unreachable!("replaced by ffi")
//}
//
//#[ffi]
//pub fn freestanding_returns_iterator(
//    input: impl IntoIterator<Item = u32>,
//) -> impl ExactSizeIterator<Item = u32> {
//    unreachable!("replaced by ffi")
//}
//
//// FIXME: Write a test
////#[ffi]
////pub fn freestanding_take_and_return_array(input: [(u32, u32); 2]) -> impl Into<[(u32, u32); 2]> {
////    unreachable!("replaced by ffi")
////}
//
//#[ffi]
//pub fn freestanding_take_and_return_local_transparent_ref(input: &Transparent) -> Transparent {
//    unreachable!("replaced by ffi")
//}
//
////#[ffi]
////pub fn freestanding_take_and_return_boxed_int(input: Box<u8>) -> Box<u8> {
////    unreachable!("replaced by ffi")
////}
//
//#[ffi]
//pub fn freestanding_take_and_return_boxed_int_ref(input: &Box<u8>) -> &Box<u8> {
//    unreachable!("replaced by ffi")
//}
//
//#[ffi]
//pub fn freestanding_return_empty_tuple_result(flag: bool) -> Result<(), u8> {
//    unreachable!("replaced by ffi")
//}
//
//#[test]
//fn take_and_return_robust_ref() {
//    let input = Robust(420);
//    let output: &Robust = input.take_ref();
//    assert_eq!(&input, output);
//}
//
//#[test]
//fn take_and_return_non_local() {
//    let input = 420;
//    let output: &u32 = freestanding_returns_non_local(&input);
//    assert_eq!(&input, output);
//}
//
//#[test]
//fn tuple_ref_is_coppied_when_returned() {
//    let in_tuple = (420, 420);
//    let out_tuple: (u32, u32) = freestanding_returns_local_ref(&in_tuple);
//    assert_eq!(in_tuple, out_tuple);
//}
//
//#[test]
//fn vec_of_tuples_is_coppied_when_returned() {
//    let in_tuple = Box::from([(420_u32, 420_u32)]);
//    let out_tuple: Box<[(u32, u32)]> = freestanding_returns_local_slice(&in_tuple);
//    assert_eq!(in_tuple, out_tuple);
//}
//
//#[test]
//fn boxed_slice_of_primitives() {
//    let in_boxed_slice = vec![420_u32, 420_u32].into_boxed_slice();
//    let out_boxed_slice: Box<[u32]> = freestanding_returns_boxed_slice(in_boxed_slice.clone());
//    assert_eq!(in_boxed_slice, out_boxed_slice);
//}
//
//#[test]
//fn return_iterator() {
//    let input = vec![420_u32, 420_u32];
//    let output = freestanding_returns_iterator(input.clone());
//    assert_eq!(input, output);
//}
//
//// FIXME: Check previous comment
////#[test]
////fn take_and_return_array() {
////    let input = [(420, 420), (420, 420)];
////    let output: [(u32, u32); 2] = freestanding_take_and_return_array(input);
////    assert_eq!(input, output);
////}
//
//#[test]
//fn take_and_return_transparent_local_ref() {
//    let input = Transparent((420, 420));
//    let output: Transparent = freestanding_take_and_return_local_transparent_ref(&input);
//    assert_eq!(input, output);
//}
//
////#[test]
////fn take_and_return_boxed_int() {
////    let input: Box<u8> = Box::new(42u8);
////    let output: Box<u8> = freestanding_take_and_return_boxed_int(input.clone());
////    assert_eq!(input, output);
////}
//
//#[test]
//fn take_and_return_boxed_int_ref() {
//    let input: Box<u8> = Box::new(42u8);
//    let output: &Box<u8> = freestanding_take_and_return_boxed_int_ref(&input);
//    assert_eq!(input, *output);
//}
//
//#[test]
//fn return_empty_tuple_result() {
//    assert!(freestanding_return_empty_tuple_result(false).is_ok());
//}
//
//mod ffi {
//    use std::alloc;
//
//    use co3::{
//        ExternC, (), CTuple2,
//        out_ptr::OutPtr,
//        slice::{CBoxedSlice, CSliceMut, CSlice},
//    };
//
//    co3::def_fns! { dealloc }
//
//    #[unsafe(export_name = "Robust__take_ref")]
//    unsafe extern "C" fn Robust__take_ref(
//        input: *const super::Robust,
//        output: *mut *const super::Robust,
//    ) {
//        unsafe {
//            output.write(input);
//        }
//
//        ()
//    }
//
//    #[unsafe(export_name = "__freestanding_returns_non_local")]
//    unsafe extern "C" fn __freestanding_returns_non_local(
//        input: *const u32,
//        output: *mut *const u32,
//    ) {
//        unsafe {
//            output.write(input);
//        }
//
//        ()
//    }
//
//    #[unsafe(export_name = "__freestanding_returns_local_ref")]
//    unsafe extern "C" fn __freestanding_returns_local_ref(
//        input: *const CTuple2<u32, u32>,
//        output: *mut CTuple2<u32, u32>,
//    ) {
//        unsafe {
//            output.write(input.read());
//        }
//
//        ()
//    }
//
//    #[unsafe(export_name = "__freestanding_returns_local_slice")]
//    unsafe extern "C" fn __freestanding_returns_local_slice(
//        input: CSlice<CTuple2<u32, u32>>,
//        output: *mut CBoxedSlice<CTuple2<u32, u32>>,
//    ) {
//        unsafe {
//            let input = input.into_rust().map(Into::into);
//            output.write(CBoxedSlice::from_boxed_slice(input));
//        }
//
//        ()
//    }
//
//    #[unsafe(export_name = "__freestanding_returns_boxed_slice")]
//    unsafe extern "C" fn __freestanding_returns_boxed_slice(
//        input: CSliceMut<u32>,
//        output: *mut CBoxedSlice<u32>,
//    ) {
//        unsafe {
//            let input = input.into_rust().map(|slice| (&*slice).into());
//            output.write(CBoxedSlice::from_boxed_slice(input));
//        }
//
//        ()
//    }
//
//    #[unsafe(export_name = "__freestanding_returns_iterator")]
//    unsafe extern "C" fn __freestanding_returns_iterator(
//        input: CSliceMut<u32>,
//        output: *mut CBoxedSlice<u32>,
//    ) {
//        unsafe {
//            let input = input.into_rust().map(|slice| (&*slice).into());
//            output.write(CBoxedSlice::from_boxed_slice(input));
//        }
//
//        ()
//    }
//
//    #[unsafe(export_name = "__freestanding_take_and_return_local_transparent_ref")]
//    unsafe extern "C" fn __freestanding_take_and_return_local_transparent_ref(
//        input: <&(u32, u32) as ExternC>::CType,
//        output: *mut <&(u32, u32) as OutPtr>::OutPtr,
//    ) {
//        unsafe {
//            output.write(input.read());
//        }
//        ()
//    }
//
////    #[unsafe(export_name = "__freestanding_take_and_return_boxed_int")]
////    unsafe extern "C" fn __freestanding_take_and_return_boxed_int(
////        input: <Box<u8> as ExternC>::CType,
////        output: *mut <Box<u8> as OutPtr>::OutPtr,
////    ) {
////        unsafe {
////            output.write(input.read());
////        }
////
////        ()
////    }
//
//    #[unsafe(export_name = "__freestanding_take_and_return_boxed_int_ref")]
//    unsafe extern "C" fn __freestanding_take_and_return_boxed_int_ref(
//        input: <&Box<u8> as ExternC>::CType,
//        output: *mut <&Box<u8> as OutPtr>::OutPtr,
//    ) {
//        unsafe {
//            output.write(input);
//        }
//
//        ()
//    }
//
//    #[unsafe(export_name = "__freestanding_return_empty_tuple_result")]
//    unsafe extern "C" fn __freestanding_return_empty_tuple_result(
//        input: <bool as ExternC>::CType,
//    ) {
//        if input == 1 {
//            return ();
//        }
//
//        ()
//    }
//}


