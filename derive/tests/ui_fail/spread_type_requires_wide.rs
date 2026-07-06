use co3::{export_C, extern_C, tuple::ReprCTuple2};

export_C! {
    fn good(value: ..ReprCTuple2<u8, u8>);
}

extern_C! {
    fn good(value: ..ReprCTuple2<u8, u8>);
}

export_C! {
    fn bad(value: ..u32);
}

extern_C! {
    fn bad(value: ..u32);
}

fn main() {}
