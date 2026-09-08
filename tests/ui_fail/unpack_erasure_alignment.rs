use co3::{ReprC, ffi, slice::UnpackAs};
use rust_spec::RustSpec;

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Abi(u32, u32);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Value(u64);

impl UnpackAs<u8, u64> for Value {
    fn into_parts(value: Self::CType) -> (u8, u64) {
        (0, value.0)
    }
}

ffi! {
    #![unsafe(extern("C"))]

    fn alignment_mismatch(
        #[unpack_as(u8, u64 => Abi)]
        move value: Value,
    );
}

fn main() {
    alignment_mismatch(Value(1));
}
