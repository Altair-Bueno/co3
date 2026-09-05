use co3::{ffi, tuple::ReprCTuple2};

ffi! {
    #![unsafe(export("C"))]

    fn missing_spread(#[spread(_, _)] value: u32);
    fn missing_spread_try(#[try_spread(_, _)] value: u32);
}

ffi! {
    #![unsafe(extern("C"))]

    fn missing_spread(#[spread(_, _)] value: u32);
    fn missing_spread_try(#[try_spread(_, _)] value: u32);
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

    fn missing_spread<dyn(u8) T>(#[spread(_, _)] value: &[T])
    where
        use<T> @ <u32>;

    fn missing_spread_try<dyn(u8) T>(#[try_spread(_, _)] value: &[T])
    where
        use<T> @ <u32>;

    fn missing_payload_spread<dyn(u8) T = (u8, u8)>(#[spread(_, _)] value: T)
    where
        use<T> @ <(u8, u8)>;

    fn missing_payload_spread_try<dyn(u8) T = (u8, u8)>(#[try_spread(_, _)] value: T)
    where
        use<T> @ <(u8, u8)>;
}

fn main() {}
