use co3::{export_C, extern_C};

export_C! {
    fn bad(value: ..u32);
}

extern_C! {
    fn bad(value: ..u32);
}

fn main() {}
