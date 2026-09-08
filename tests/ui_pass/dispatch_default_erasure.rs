use co3::{Handle, ReprC, ffi, slice::Spread2, tuple::ReprCTuple2};
use rust_spec::RustSpec;

#[derive(RustSpec, ReprC, Handle)]
#[handle(unsafe(id(u8 = 1)))]
#[repr(transparent)]
struct Value(u16);

#[derive(RustSpec, ReprC, Handle)]
#[handle(unsafe(id(u8 = 2)))]
#[repr(transparent)]
struct Pair(ReprCTuple2<u8, u8>);

impl Spread2<u16, u16> for Pair {
    fn into_parts(value: Self::CType) -> (u16, u16) {
        (value.0.0.into(), value.0.1.into())
    }
}

mod symbols {
    #[unsafe(no_mangle)]
    extern "C" fn erased(_: u8, _: u16) {}

    #[unsafe(no_mangle)]
    extern "C" fn spread_wins(_: u8, _: u16, _: u16) {}
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "erased"]
    fn erased<dyn(u8) T = u16>(move value: T)
    where
        use<T> @ <Value>;

    #[symbol_name = "spread_wins"]
    fn spread_wins<dyn(u8) T = (u16, u16)>(#[spread(u16, u16)] move value: T)
    where
        use<T> @ <Pair>;
}

fn main() {
    erased(Value(2));
    spread_wins(Pair(ReprCTuple2(1, 2)));
}
