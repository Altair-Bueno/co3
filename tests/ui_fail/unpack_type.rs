use co3::{ffi, tuple::ReprCTuple2};

ffi! {
    #![unsafe(export("C"))]

    fn missing_unpack(#[unpack(_, _)] value: u32);
    fn missing_unpack_try(#[unpack(_, _)] value: u32);
    fn missing_second_unpack(#[unpack(u8, _)] value: u32);
    fn missing_second_unpack_try(#[unpack(u8, _)] value: u32);
}

ffi! {
    #![unsafe(extern("C"))]

    fn missing_unpack(#[unpack(_, _)] value: u32);
    fn missing_unpack_try(#[unpack(_, _)] value: u32);
}

ffi! {
    #![unsafe(extern("C"))]

    fn missing_types(#[unpack] value: ReprCTuple2<u8, u8>);
    fn missing_types_try(#[unpack] value: ReprCTuple2<u8, u8>);
    fn missing_erased_type(#[unpack(u8 => _, u8)] value: ReprCTuple2<u8, u8>);
    fn nested_option(#[unpack(_, _)] value: Option<Option<&[u8]>>);
}

ffi! {
    #![unsafe(extern("C"))]

    fn missing_into(#[unpack(u32, String)] value: ReprCTuple2<u8, u8>);
    fn missing_into_try(#[unpack(u32, String)] value: ReprCTuple2<u8, u8>);
}

ffi! {
    #![unsafe(extern("C"))]

    fn missing_unpack<dyn(u8) T>(#[unpack(_, _)] value: &[T])
    where
        use<T> @ <u32>;

    fn missing_unpack_try<dyn(u8) T>(#[unpack(_, _)] value: &[T])
    where
        use<T> @ <u32>;

    fn missing_payload_unpack<dyn(u8) T = (u8, u8)>(#[unpack(_, _)] value: T)
    where
        use<T> @ <(u8, u8)>;

    fn missing_payload_unpack_try<dyn(u8) T = (u8, u8)>(#[unpack(_, _)] value: T)
    where
        use<T> @ <(u8, u8)>;
}

fn main() {}
