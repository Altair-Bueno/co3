use co3::{export_C, extern_C};

fn fallible(flag: bool) -> Result<u8, u16> {
    if flag { Ok(42) } else { Err(7) }
}

export_C! {
    #![export(crate = "out_ptr_result")]

    fn fallible(flag: bool) -> Result<u8, u16>;
}

extern_C! {
    #![symbol_prefix = "out_ptr_result"]

    fn imported_fallible(flag: bool) -> Result<u8, u16>;
}

fn main() {}
