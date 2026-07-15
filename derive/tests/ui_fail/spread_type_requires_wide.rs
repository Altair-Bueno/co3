use co3::{ffi, tuple::ReprCTuple2};

ffi! {
    #![unsafe(export("C"))]

    fn good(value: ..ReprCTuple2<u8, u8>);
}

ffi! {
    #![unsafe(extern("C"))]

    fn good(value: ..ReprCTuple2<u8, u8>);
}

ffi! {
    #![unsafe(export("C"))]

    fn bad(value: ..u32);
}

ffi! {
    #![unsafe(extern("C"))]

    fn bad(value: ..u32);
}

fn main() {}
