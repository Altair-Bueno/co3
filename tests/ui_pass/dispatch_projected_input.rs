use core::mem::MaybeUninit;

use co3::{Tag, ffi};

trait Field {
    type Buffer;
}

#[derive(Tag)]
#[tag(u16, unsafe(1))]
enum NumericField {}

impl Field for NumericField {
    type Buffer = isize;
}

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u16, unsafe(1))]
    type Statement;

    impl Statement {
        fn get_field<dyn(u16) A: Field>(
            &self,
            field: <dyn A>::TAG,
            #[unpack(*mut core::ffi::c_void)]
            output: Option<&mut MaybeUninit<<A as Field>::Buffer>>,
        );

        fn try_get_field<dyn(u16) A: Field>(
            &self,
            field: <dyn A>::TAG,
            #[unpack(*mut core::ffi::c_void)]
            output: Option<&mut MaybeUninit<<A as Field>::Buffer>>,
        );
    }
}

fn main() {}
