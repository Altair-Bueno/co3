use co3::{ReprC};
use rust_spec::RustSpec;

#[derive(RustSpec, ReprC)]
#[reprC(is_valid = |a| *a != 42)]
#[rust_spec(with_custom_niche)]
#[reprC(NICHE_VALUE = Self::CType {
    field: 42
})]
pub struct CustomStructValid {
    field: u32,
}

#[derive(RustSpec, ReprC)]
pub enum CustomEnumValid {
    #[reprC(is_valid = |a| *a != 0)]
    A(u32),
    B,
}

#[derive(ReprC)]
pub struct CustomStructNotValid {
    #[reprC(is_valid = |a| *a != 0)]
    field: u32,
}

#[derive(ReprC)]
#[reprC(NICHE_VALUE = 42)]
pub enum CustomEnum1 {
    A(u32),
    B,
}

#[derive(ReprC)]
#[reprC(is_valid = |a| *a != 0)]
pub enum CustomEnum2 {
    A(u32),
    B,
}

#[derive(ReprC)]
#[reprC(is_valid = |a| *a != 0)]
pub union CustomUnion1 {
    a: u32,
}

#[derive(ReprC)]
#[reprC(is_valid = |a| *a != 0)]
pub union CustomUnion2 {
    a: u32,
}

#[derive(ReprC)]
#[reprC(NICHE_VALUE = unsafe { core::mem::zeroed() })]
pub struct Parametrized<T: ?Sized>(T);

#[derive(ReprC)]
#[reprC(NICHE_VALUE = unsafe { core::mem::zeroed() })]
pub struct UnsizedSlice<T>([T]);

fn unsized_niches_cannot_be_used() {
    fn require_niche<T: co3::niche::Niche>() {}
    require_niche::<UnsizedSlice<u8>>();
    require_niche::<UnsizedStr>();
}

fn main() {}
