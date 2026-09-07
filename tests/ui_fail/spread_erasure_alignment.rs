use co3::{ReprC, ffi, slice::Spread2};
use rust_spec::RustSpec;

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Abi(u32, u32);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Value(u64);

impl Spread2<u8, u64> for CValue {
    fn into_parts(self) -> (u8, u64) {
        (0, self.0)
    }
}

ffi! {
    #![unsafe(extern("C"))]

    fn alignment_mismatch(
        #[spread(u8, u64 => Abi)]
        move value: Value,
    );
}

fn main() {
    alignment_mismatch(Value(1));
}
