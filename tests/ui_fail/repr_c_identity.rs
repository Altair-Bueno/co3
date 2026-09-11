use co3::ReprC;

#[derive(ReprC)]
#[reprC(identity)]
struct MissingRepr(i32);

#[derive(ReprC)]
#[reprC(identity)]
#[repr(u8)]
enum Enum {
    Value,
}

#[derive(ReprC)]
#[reprC(identity)]
#[repr(C)]
struct NonRobust(bool);

fn main() {}
