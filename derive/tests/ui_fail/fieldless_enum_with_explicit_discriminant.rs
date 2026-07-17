use co3::{family::TypeFamily, ReprC};

#[derive(TypeFamily, ReprC)]
#[repr(u8)]
pub enum EnumWithExplicitDiscriminant {
    A = 1,
    B,
    C,
    D,
}

fn main() {}
