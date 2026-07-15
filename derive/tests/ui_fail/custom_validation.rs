use co3::ReprC;

#[derive(ReprC)]
#[reprC(is_valid = |a| *a != 42)]
#[reprC(NICHE_VALUE = Self::CType {
    field: 42
})]
pub struct CustomStructValid {
    field: u32,
}

#[derive(ReprC)]
pub enum CustomEnumValid {
    #[reprC(is_valid = |a| *a != 0)]
    A(u32),
    B
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
    B
}

#[derive(ReprC)]
#[reprC(is_valid = |a| *a != 0)]
pub union CustomUnion1 {
    a: u32
}

#[derive(ReprC)]
#[reprC(is_valid = |a| *a != 0)]
pub union CustomUnion2 {
    a: u32
}

#[derive(ReprC)]
#[reprC(NICHE_VALUE = unsafe {
    core::mem::zeroed()
})]
pub struct Parametrized<T: ?Sized>(T);

#[derive(ReprC)]
#[reprC(NICHE_VALUE = Self::CType(0))]
pub struct UnsizedSlice<T>([T]);

#[derive(ReprC)]
#[reprC(NICHE_VALUE = Self::CType(""))]
pub struct UnsizedStr(str);

#[derive(ReprC)]
#[reprC(NICHE_VALUE = Self::CType(0))]
pub struct UnsizedTraitObject(dyn Send);

fn main() {}
