use co3::ffi;

ffi! {
    #![unsafe(export("C"))]
    #![symbol_fragments(u8 = "Byte")]

    fn exported();
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_fragments(1 = "One")]

    fn literal_exported();
}

fn main() {}
