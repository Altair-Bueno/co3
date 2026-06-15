use co3::ReprC;

#[derive(ReprC)]
#[reprC(NICHE_VALUE = 42)]
#[repr(u8)]
pub enum PrimitiveFieldless {
    A,
    B,
}

fn main() {}
