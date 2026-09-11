use co3::{Tag, ReprC, ffi};
use rust_spec::RustSpec;

#[derive(RustSpec, ReprC, Tag)]
#[tag(unsafe(id(u8 = 1)))]
#[repr(transparent)]
struct Value(u8);

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "layout_mismatch"]
    fn layout_mismatch<dyn(u8) T = u16>(move value: T)
    where
        use<T> @ <Value>;
}

fn main() {
    layout_mismatch(Value(1));
}
