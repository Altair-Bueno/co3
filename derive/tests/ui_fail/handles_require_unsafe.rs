use co3::{handle::HandleFamily, handles};

struct Opaque;

impl HandleFamily for Opaque {
    type Kind = u8;
}

handles! {
    Opaque,
}

fn main() {}
