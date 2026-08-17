use co3::{ffi, tuple::ReprCTuple2};

ffi! {
    #![unsafe(extern("C"))]

    fn explicit_non_dispatch(#[spread2(u32, i32)] value: ReprCTuple2<u8, u8>);
}

ffi! {
    #![unsafe(extern("C"))]

    fn good(#[spread2] value: ReprCTuple2<u8, u8>);
}

ffi! {
    #![unsafe(extern("C"))]

    fn bad(#[spread2] value: u32);
}

ffi! {
    #![unsafe(export("C"))]

    fn good(#[spread2] value: u32);
}

fn main() {}
