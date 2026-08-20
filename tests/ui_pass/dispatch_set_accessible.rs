use co3::{Handle, ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, Handle, ReprC)]
#[handle(unsafe(id(u8 = 1)))]
#[repr(transparent)]
struct First(u8);

ffi! {
    #![unsafe(extern("C"))]

    pub fn accessible<dyn(u8) T = First>(value: T)
    where
        use<T> @ (<First>);
}

fn assert_dispatch_set<T>()
where
    (T,): accessibleDispatchSet,
{
}

fn main() {
    assert_dispatch_set::<First>();
}
