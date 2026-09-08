use co3::{ReprC, ffi, slice::UnpackAs};
use rust_spec::RustSpec;

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Logical(u16);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Value(u16);

impl UnpackAs<u8, CLogical> for Value {
    fn into_parts(value: Self::CType) -> (u8, CLogical) {
        (0, CLogical(value.0))
    }
}

ffi! {
    #![unsafe(extern("C"))]

    fn size_mismatch(
        #[unpack_as(u8, Logical => u32)]
        move value: Value,
    );
}

fn main() {
    size_mismatch(Value(1));
}
