use co3::{Handle, ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, Handle, ReprC)]
#[handle(unsafe(id(u8 = 1)))]
#[repr(transparent)]
struct First(u8);

#[derive(RustSpec, Handle, ReprC)]
#[handle(unsafe(id(u8 = 2)))]
#[repr(transparent)]
struct Second(u8);

#[derive(RustSpec, Handle, ReprC)]
#[handle(unsafe(id(u8 = 3)))]
#[repr(transparent)]
struct Other(u8);

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "sealed_dispatch"]
    fn sealed<dyn(u8) T = u8>(move value: T)
    where
        use<T> @ (<First> | <Second>);
}

impl crate::sealedDispatchSet for (Other,) {}

fn main() {}
