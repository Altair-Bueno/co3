use co3::{ReprC, ffi, handles, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[reprC(id(u8))]
#[repr(transparent)]
struct First(u8);

#[derive(RustSpec, ReprC)]
#[reprC(id(u8))]
#[repr(transparent)]
struct Second(u8);

#[derive(RustSpec, ReprC)]
#[reprC(id(u8))]
#[repr(transparent)]
struct Other(u8);

handles! {
    unsafe {
        First,
        Second,
        Other,
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "sealed_dispatch"]
    fn sealed<dyn(u8) T = u8>(move value: T)
    where
        use<T> @ (<First> | <Second>);
}

impl __Co3DispatchSet_sealed for (Other,) {}

fn main() {}
