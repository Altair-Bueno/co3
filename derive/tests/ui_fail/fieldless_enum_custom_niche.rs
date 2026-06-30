use co3::ReprC;

#[derive(ReprC)]
#[reprC(NICHE_VALUE = 42)]
pub enum PrimitiveFieldless1 {
    A,
    B,
}

#[derive(ReprC)]
#[reprC(is_valid = |a| {
    a != 0
})]
pub enum PrimitiveFieldless2 {
    A,
    B,
}

fn main() {}
