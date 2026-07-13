use co3::{ReprC, ffi, out_ptr::StatusReturn};

#[derive(ReprC)]
#[repr(transparent)]
struct CustomReturn(u8);

impl StatusReturn for CustomReturn {
    fn ok() -> Self {
        unimplemented!()
    }
    fn execution_fail() -> Self {
        unimplemented!()
    }
    fn unrecoverable_error() -> Self {
        unimplemented!()
    }
    fn trap_representation() -> Self {
        unimplemented!()
    }
    fn unknown_handle() -> Self {
        unimplemented!()
    }
    fn is_ok(self) -> bool {
        unimplemented!()
    }
}

mod provider {
    use super::*;

    fn direct_return_panic() -> u8 {
        0
    }
    fn direct_return_abort() -> u8 {
        0
    }
    fn direct_custom_return() -> CustomReturn {
        CustomReturn(0)
    }

    fn out_ptr_return_panic() -> u8 {
        0
    }

    fn out_ptr_return_abort() -> u8 {
        0
    }

    fn out_ptr_custom_return() -> u8 {
        0
    }

    ffi! {
        #![export("C")]
        #![panic = "abort"]

        fn direct_return_abort() -> u8;
    }
    ffi! {
        #![export("C")]

        fn direct_return_panic() -> u8;
    }
    ffi! {
        #![export("C")]

        fn direct_custom_return() -> CustomReturn;
    }

    ffi! {
        #![export("C")]
        #![panic = "abort"]

        fn out_ptr_return_abort(#[out_ptr] out_ptr: &mut u8);
    }
    ffi! {
        #![export("C")]

        fn out_ptr_return_panic(#[out_ptr] out_ptr: &mut u8);
    }
    ffi! {
        #![export("C")]

        fn out_ptr_custom_return(#[out_ptr] out_ptr: &mut u8) -> CustomReturn;
    }
}

ffi! {
    #![extern("C")]
    #![panic = "abort"]

    fn direct_return_abort() -> u8;
}
ffi! {
    #![extern("C")]

    fn direct_return_panic() -> u8;
}
ffi! {
    #![extern("C")]

    fn direct_custom_return() -> CustomReturn;
}

ffi! {
    #![extern("C")]
    #![panic = "abort"]

    fn out_ptr_return_abort(#[out_ptr] out_ptr: &mut u8);
}
ffi! {
    #![extern("C")]

    fn out_ptr_return_panic(#[out_ptr] out_ptr: &mut u8);
}
ffi! {
    #![extern("C")]

    fn out_ptr_custom_return(#[out_ptr] out_ptr: &mut u8) -> CustomReturn;
}

fn main() {}
