use co3::{rust_spec::RustSpec, ReprC};

#[derive(RustSpec, ReprC)]
#[repr(u8)]
pub enum EnumWithExplicitDiscriminant {
    A = 1,
    B,
    C,
    D,
}

fn main() {}
