use co3::{ffi, handles, rust_spec::RustSpec, ReprC};

#[derive(RustSpec, ReprC)]
#[reprC(unsafe(id(u8)))]#[repr(transparent)]
struct Attribute(u32);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Prefix<T: ?Sized> {
    head: u32,
    tail: T,
}

handles! {
    unsafe {
        Attribute,
    }
}

fn prefix_head<T: ?Sized>(prefix: &Prefix<T>) -> u32 {
    prefix.head
}

ffi! {
    #![unsafe(export("C"))]

    #[explicit_lifetimes]
    #[symbol_name = "dispatch_opaque_tail_head"]
    fn prefix_head<'a, dyn(u8) T: ?Sized>(prefix: &'a Prefix<T>) -> u32
    where
        use<T> @ <Attribute>;
}

ffi! {
    #![unsafe(extern("C"))]

    #[explicit_lifetimes]
    #[symbol_name = "dispatch_opaque_tail_head"]
    fn imported_prefix_head<'a, dyn(u8) T: ?Sized>(
        handle_id: <dyn T>::ID,
        prefix: &'a Prefix<T>,
    ) -> u32
    where
        use<T> @ <Attribute>;
}

fn main() {
    let prefix = Box::leak(Box::new(Prefix {
        head: 7,
        tail: Attribute(9),
    }));

    assert_eq!(imported_prefix_head(prefix), 7);
}
