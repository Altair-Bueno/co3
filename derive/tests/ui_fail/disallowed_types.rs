use co3::{ReprC, extern_C, handles};

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
    use co3::export_C;

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

    export_C! {
        type MyType;

        impl ToOwned for Box<MyType> {
            #[unsafe(export_name = "my_type_new")]
            fn to_owned(&self) -> <Self as ToOwned>::Owned;
        }

        pub extern "C" fn disallowed_types_by_ref(arg1: (), arg2: [u8; 2], move arg3: (), move arg4: [u8; 2]);
        pub extern "C" fn disallowed_types_by_val(move arg1: (), move arg2: [u8; 2]) -> [u8; 2];
        pub extern "C" fn disallowed_opaque_types(move arg1: Box<MyType>, arg2: Box<MyType>);
    }

    export_C! {
        #[dispatch(<Handle>)]
        impl<dyn(usize) T = Array> Dispatch for T {
            fn me(self);
        }
    }

    export_C! {
        #[dispatch(<Handle2>)]
        impl<dyn(usize) T = Array2> Dispatch for T {
            fn me(self);
        }
    }
}

extern_C! {
    #![link(crate = "kita")]

    type MyType;

    impl ToOwned for MyType {
        type Owned = OwnedMyType;

        #[link_name = "my_type_new"]
        fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }

    pub extern "C" fn disallowed_types_by_ref(arg1: (), arg2: [u8; 2]);
    pub extern "C" fn disallowed_types_by_val(move arg1: (), move arg2: [u8; 2]) -> [u8; 2];
    pub extern "C" fn disallowed_opaque_types(move arg1: OwnedMyType, arg2: OwnedMyType);
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<Array>)]
    impl<dyn(usize) T = Handle> Dispatch for T {
        fn me(id: <dyn T>::ID, self);
    }

    #[dispatch(<Array2>)]
    impl<dyn(usize) T = Handle2> Dispatch for T {
        fn me(id: <dyn T>::ID, self);
    }
}

fn main() {}
