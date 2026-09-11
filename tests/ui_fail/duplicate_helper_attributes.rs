use co3::ffi;

ffi! {
    #![unsafe(export("C"))]

    #[symbol_name = "first"]
    #[symbol_name = "second"]
    fn duplicate_symbol();

}

ffi! {
    #![unsafe(extern("C"))]

    fn duplicate_unpack(
        #[unpack(u8, u8)]
        #[unpack(u8, u8)]
        value: u32,
    );
}

fn main() {}
