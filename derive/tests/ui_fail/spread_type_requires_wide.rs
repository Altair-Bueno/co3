use co3::{ffi, tuple::ReprCTuple2};

ffi! {
    #![export("C")]

    fn good(value: ..ReprCTuple2<u8, u8>);
}

ffi! {
    #![extern("C")]

    fn good(value: ..ReprCTuple2<u8, u8>);
}

ffi! {
    #![export("C")]

    fn bad(value: ..u32);
}

ffi! {
    #![extern("C")]

    fn bad(value: ..u32);
}

fn main() {}
