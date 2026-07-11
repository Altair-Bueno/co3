use co3::ffi;

co3::handles! {
    FfiStruct,
}

trait Kita {
    type T;

    extern "C" fn kita1(self);
}

ffi! {
    #![export("C")]

    #[derive(Clone)]
    enum FfiStruct {
        A,
        B,
    }
}

ffi! {}

ffi! {
    #![export("C")]
    #![export("C")]
}

ffi! {
    #![export("C")]
    #![extern("C")]
}

ffi! {
    #![export("C")]

    #![feature(generic_const_exprs)]
}

ffi! {
    #![export("C")]

    #![feature(extern_types)]
    #![feature(extern_types)]
}

ffi! {
    #![export("C")]

    trait Kita {
        fn kita(self);
    }
}

ffi! {
    #![export("C")]

    enum Kita {}
}

ffi! {
    #![export("C")]

    struct Kita {}
}

ffi! {
    #![export("C")]

    union Kita {}
}

ffi! {
    #![export("C")]

    #[unknown_attribute]
    fn kita3(_a: u32);
}

ffi! {
    #![export("C")]

    #[some_attr]
    type OpaqueType;
}

ffi! {
    #![export("C")]

    #[some_attr]
    impl Clone for FfiStruct {
        fn clone(&self) -> Self;
    }
}

ffi! {
    #![export("C")]

    impl Kita for u32 {
        #[dispatch]
        fn kita(self);
    }
}

ffi! {
    #![export("C")]

    #[dispatch]
    type OpaqueType;
}

ffi! {
    #![export("C")]

    impl Kita for u32 {
        fn kita1(self) {}
    }
}

ffi! {
    #![export("C")]

    fn kita1(a: u32) {}
}

ffi! {
    #![export("C")]

    fn kita1((a, b): (u32, u32));
}

ffi! {
    #![export("C")]

    #[id(u32)]
    type OpaqueType<T>;

    #[dispatch(<u32>)]
    impl<T> Drop for dyn OpaqueType<T> {
        fn drop(&mut self);
    }

    #[dispatch]
    impl<dyn(u32) T> Clone for OpaqueType<T> {
        fn clone(&self);
    }
}

ffi! {
    #![export("C")]

    #[id(u32)]
    type OpaqueType<T>;

    #[dispatch(<u32>)]
    impl<T> Drop for dyn OpaqueType<T> {
        fn drop(&mut self);
    }

    #[dispatch(<u32, u8>)]
    impl<dyn(u32) T> Clone for OpaqueType<T> {
        fn clone(&self);
    }
}

fn main() {}
