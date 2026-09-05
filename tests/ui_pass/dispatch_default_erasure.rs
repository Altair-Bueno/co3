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

impl Spread2 for CPair {
    type Part1 = u8;
    type Part2 = u8;

    fn into_parts(self) -> (Self::Part1, Self::Part2) {
        (self.0.0, self.0.1)
    }

    fn from_parts(part1: Self::Part1, part2: Self::Part2) -> Self {
        Self(ReprCTuple2(part1, part2))
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
