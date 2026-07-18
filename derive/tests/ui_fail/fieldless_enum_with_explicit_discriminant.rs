use co3::{rust_spec::TypeSpec, ReprC};

#[derive(TypeSpec, ReprC)]
#[repr(u8)]
pub enum EnumWithExplicitDiscriminant {
    A = 1,
    B,
    C,
    D,
}

fn main() {}
