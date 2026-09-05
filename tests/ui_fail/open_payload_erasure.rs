use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    fn erased<dyn(u8) T = u32>(move value: T);
}

fn main() {}
