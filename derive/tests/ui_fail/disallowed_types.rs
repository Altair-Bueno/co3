use co3::{rust_spec::RustSpec, ReprC, ffi, handles};

trait Dispatch {
    fn me(self);
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[reprC(id(usize))]
struct Handle(usize);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[reprC(id(usize))]
struct Handle2(u64);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[reprC(id(usize))]
struct Array([u8; 2]);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[reprC(id(usize))]
struct Array2([u8; 8]);

handles! {
    unsafe {
        Handle,
        Handle2,
        Array,
        Array2,
    }
}

mod provider {
    use co3::ffi;

    use super::*;

    #[derive(Clone)]
    struct OpaqueZst;

    impl Dispatch for Handle {
        fn me(self) {}
    }
    impl Dispatch for Handle2 {
        fn me(self) {}
    }

    #[expect(unused_variables)]
    pub extern "C" fn disallowed_args_by_ref(arg1: (), arg2: [u8; 2]) {}

    #[expect(unused_variables)]
    pub extern "C" fn disallowed_args_by_val(arg1: (), arg2: [u8; 2]) {}

    #[expect(unused_variables)]
    pub extern "C" fn disallowed_return1() -> [u8; 2] {
        [42, 42]
    }

    #[expect(unused_variables)]
    pub extern "C" fn disallowed_return2() -> Box<[u8]> {
        Box::new([42, 42])
    }

    #[expect(unused_variables)]
    pub extern "C" fn disallowed_opaque_args(arg1: Box<OpaqueZst>) {}
    pub extern "C" fn disallowed_opaque_return() -> Box<OpaqueZst> {
        Box::new(OpaqueZst)
    }

    ffi! {
        #![unsafe(export("C"))]

        type OpaqueZst;

        impl ToOwned for Box<OpaqueZst> {
            #[symbol_name = "my_type_new"]
            fn to_owned(&self) -> <Self as ToOwned>::Owned;
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        pub extern "C" fn disallowed_args_by_ref(arg1: (), arg2: [u8; 2]);
    }
    ffi! {
        #![unsafe(export("C"))]

        pub extern "C" fn disallowed_args_by_val(move arg1: (), move arg2: [u8; 2]);
    }

    ffi! {
        #![unsafe(export("C"))]

        pub extern "C" fn disallowed_return1() -> [u8; 2];
    }
    ffi! {
        #![unsafe(export("C"))]

        pub extern "C" fn disallowed_return2() -> Box<[u8]>;
    }

    ffi! {
        #![unsafe(export("C"))]

        pub extern "C" fn disallowed_opaque_args(arg1: Box<OpaqueZst>);
    }
    ffi! {
        #![unsafe(export("C"))]

        pub extern "C" fn disallowed_opaque_return() -> Box<OpaqueZst>;
    }

    ffi! {
        #![unsafe(export("C"))]

        #[erased(<Handle>)]
        impl<dyn(usize) T = Array> Dispatch for T {
            fn me(self);
        }
    }
    ffi! {
        #![unsafe(export("C"))]

        #[erased(<Handle2>)]
        impl<dyn(usize) T = Array2> Dispatch for T {
            fn me(self);
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "kita"]

    type MyType;

    impl ToOwned for MyType {
        type Owned = OwnedMyType;

        #[symbol_name = "my_type_new"]
        move fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    pub extern "C" fn disallowed_args_by_ref(arg1: (), arg2: [u8; 2]);
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    pub extern "C" fn disallowed_args_by_val(move arg1: (), move arg2: [u8; 2]);
}

ffi! {
    #![unsafe(extern("C"))]

    pub extern "C" fn disallowed_return1() -> [u8; 2];
}
ffi! {
    #![unsafe(extern("C"))]

    pub extern "C" fn disallowed_return2() -> Box<[u8]>;
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    pub extern "C" fn disallowed_opaque_args(arg1: OwnedMyType);
}
ffi! {
    #![unsafe(extern("C"))]

    pub extern "C" fn disallowed_opaque_return() -> OwnedMyType;
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    #[erased(<Array>)]
    impl<dyn(usize) T = Handle> Dispatch for T {
        fn me(id: <dyn T>::ID, self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    #[erased(<Array2>)]
    impl<dyn(usize) T = Handle2> Dispatch for T {
        fn me(id: <dyn T>::ID, self);
    }
}

fn main() {}
