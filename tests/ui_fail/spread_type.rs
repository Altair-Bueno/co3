use co3::{ffi, tuple::ReprCTuple2};

ffi! {
    #![unsafe(export("C"))]

    fn not_supported(#[spread(_, _)] value: u32);
    fn not_supported_try(#[try_spread(_, _)] value: u32);
}

ffi! {
    #![unsafe(extern("C"))]

    fn missing_types(#[spread] value: ReprCTuple2<u8, u8>);
    fn missing_types_try(#[try_spread] value: ReprCTuple2<u8, u8>);
}

ffi! {
    #![unsafe(extern("C"))]

    fn missing_into(#[spread(u32, String)] value: ReprCTuple2<u8, u8>);
    fn missing_into_try(#[try_spread(u32, String)] value: ReprCTuple2<u8, u8>);
}

ffi! {
    #![unsafe(extern("C"))]

    fn missing_spread(#[spread(_, _)] value: u32);
    fn missing_spread_try(#[try_spread(_, _)] value: u32);
}

fn main() {}
