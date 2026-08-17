use co3::{ReprC, ffi, handles, rust_spec::RustSpec};

trait Kind {
    fn code() -> u8;
}

#[derive(RustSpec, ReprC)]
#[reprC(unsafe(id(u8)))]#[repr(transparent)]
struct First(u8);

#[derive(RustSpec, ReprC)]
#[reprC(unsafe(id(u8)))]#[repr(transparent)]
struct Second(u8);

impl Kind for First {
    fn code() -> u8 {
        1
    }
}

impl Kind for Second {
    fn code() -> u8 {
        2
    }
}

handles! {
    unsafe {
        First,
        Second,
    }
}

fn type_directed<T: Kind>() -> u8 {
    T::code()
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Host(u8);

ffi! {
    #![unsafe(export("C"))]

    fn type_directed<dyn(u8) T: Kind = u8>() -> u8
    where
        use<T> @ (<First> | <Second>);
}

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "dispatch_type_directed"]

    pub fn unused_generic<dyn(u8) T = u8, U>()
    where
        use<T> @ (<First> | <Second>);

    impl Host {
        pub fn unused_generic_method<dyn(u8) T = u8, U>(&self)
        where
            use<T> @ (<First> | <Second>);
    }
}

fn main() {}
