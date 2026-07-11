use co3::{ReprC, ffi, handles};

trait Dispatch {
    fn me(self);
}

#[derive(ReprC)]
#[repr(transparent)]
#[reprC(id(usize))]
struct Handle(usize);

#[derive(ReprC)]
#[repr(transparent)]
#[reprC(id(usize))]
struct Handle2(u64);

#[derive(ReprC)]
#[repr(transparent)]
#[reprC(id(usize))]
struct Array([u8; 2]);

#[derive(ReprC)]
#[repr(transparent)]
#[reprC(id(usize))]
struct Array2([u8; 8]);

handles! {
    Handle,
    Handle2,
    Array,
    Array2,
}

mod provider {
    use co3::ffi;

    use super::*;

    #[derive(Clone)]
    struct MyType;

    impl Dispatch for Handle {
        fn me(self) {}
    }
    impl Dispatch for Handle2 {
        fn me(self) {}
    }

    #[allow(unused_variables)]
    pub extern "C" fn disallowed_types_by_ref(arg1: (), arg2: [u8; 2], arg3: (), arg4: [u8; 2]) {}

    #[allow(unused_variables)]
    pub extern "C" fn disallowed_types_by_val(arg1: (), arg2: [u8; 2]) -> [u8; 2] {
        arg2
    }

    #[allow(unused_variables)]
    pub extern "C" fn disallowed_opaque_types(arg1: Box<MyType>, arg2: Box<MyType>) {}

    ffi! {
        #![export("C")]

        type MyType;

        impl ToOwned for Box<MyType> {
            #[symbol_name = "my_type_new"]
            fn to_owned(&self) -> <Self as ToOwned>::Owned;
        }
    }

    ffi! {
        #![export("C")]

        pub extern "C" fn disallowed_types_by_ref(arg1: (), arg2: [u8; 2], move arg3: (), move arg4: [u8; 2]);
    }
    ffi! {
        #![export("C")]

        pub extern "C" fn disallowed_types_by_val(move arg1: (), move arg2: [u8; 2]) -> [u8; 2];
    }
    ffi! {
        #![export("C")]

        pub extern "C" fn disallowed_opaque_types(move arg1: Box<MyType>, arg2: Box<MyType>);
    }

    ffi! {
        #![export("C")]

        #[dispatch(<Handle>)]
        impl<dyn(usize) T = Array> Dispatch for T {
            fn me(self);
        }
    }

    ffi! {
        #![export("C")]

        #[dispatch(<Handle2>)]
        impl<dyn(usize) T = Array2> Dispatch for T {
            fn me(self);
        }
    }
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    type MyType;

    impl ToOwned for MyType {
        type Owned = OwnedMyType;

        #[symbol_name = "my_type_new"]
        fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    pub extern "C" fn disallowed_types_by_ref(arg1: (), arg2: [u8; 2]);
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    pub extern "C" fn disallowed_types_by_val(move arg1: (), move arg2: [u8; 2]) -> [u8; 2];
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    pub extern "C" fn disallowed_opaque_types(move arg1: OwnedMyType, arg2: OwnedMyType);
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    #[dispatch(<Array>)]
    impl<dyn(usize) T = Handle> Dispatch for T {
        fn me(id: <dyn T>::ID, self);
    }
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    #[dispatch(<Array2>)]
    impl<dyn(usize) T = Handle2> Dispatch for T {
        fn me(id: <dyn T>::ID, self);
    }
}

fn main() {}
