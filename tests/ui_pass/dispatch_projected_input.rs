use core::mem::MaybeUninit;

use co3::{Handle, ffi};

trait Field {
    type Buffer;
}

#[derive(Handle)]
#[handle(unsafe(id(u16 = 1)))]
enum NumericField {}

impl Field for NumericField {
    type Buffer = isize;
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u16 = 1))]
    type Statement;

    impl Statement {
        fn get_field<dyn(u16) A: Field>(
            &self,
            field: <dyn A>::ID,
            #[unpack(*mut core::ffi::c_void)]
            output: Option<&mut MaybeUninit<<A as Field>::Buffer>>,
        );

        fn try_get_field<dyn(u16) A: Field>(
            &self,
            field: <dyn A>::ID,
            #[unpack(*mut core::ffi::c_void)]
            output: Option<&mut MaybeUninit<<A as Field>::Buffer>>,
        );
    }
}

fn main() {}
