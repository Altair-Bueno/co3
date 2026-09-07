use co3::{ReprC, ffi, slice::Spread2};
use rust_spec::RustSpec;

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Logical(u16);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Value(u16);

impl Spread2<u8, CLogical> for CValue {
    fn into_parts(self) -> (u8, CLogical) {
        (0, CLogical(self.0))
    }
}

ffi! {
    #![unsafe(extern("C"))]

    fn size_mismatch(
        #[spread(u8, Logical => u32)]
        move value: Value,
    );
}

fn main() {
    size_mismatch(Value(1));
}
