use co3::ffi;

co3::handles! {
    unsafe {
        FfiStruct,
    }
}

trait Kita {
    type T;

    extern "C" fn kita1(self);
}

ffi! {
    #![unsafe(export("C"))]

    #[derive(Clone)]
    enum FfiStruct {
        A,
        B,
    }
}

ffi! {}

ffi! {
    #![unsafe(export("C"))]
    #![unsafe(export("C"))]
}

ffi! {
    #![unsafe(export("C"))]
    #![unsafe(extern("C"))]
}

ffi! {
    #![unsafe(export("C"))]

    #![feature(generic_const_exprs)]
}

ffi! {
    #![unsafe(export("C"))]

    #![feature(extern_types)]
    #![feature(extern_types)]
}

ffi! {
    #![unsafe(export("C"))]

    trait Kita {
        fn kita(self);
    }
}

ffi! {
    #![unsafe(export("C"))]

    enum Kita {}
}

ffi! {
    #![unsafe(export("C"))]

    struct Kita {}
}

ffi! {
    #![unsafe(export("C"))]

    union Kita {}
}

ffi! {
    #![unsafe(export("C"))]

    #[unknown_attribute]
    fn kita3(_a: u32);
}

ffi! {
    #![unsafe(export("C"))]

    #[some_attr]
    type OpaqueType;
}

ffi! {
    #![unsafe(export("C"))]

    #[some_attr]
    impl Clone for FfiStruct {
        fn clone(&self) -> Self;
    }
}

ffi! {
    #![unsafe(export("C"))]

    impl Kita for u32 {
        #[erased]
        fn kita(self);
    }
}

ffi! {
    #![unsafe(export("C"))]

    #[erased]
    type OpaqueType;
}

ffi! {
    #![unsafe(export("C"))]

    impl Kita for u32 {
        fn kita1(self) {}
    }
}

ffi! {
    #![unsafe(export("C"))]

    fn kita1(a: u32) {}
}

ffi! {
    #![unsafe(export("C"))]

    fn kita1((a, b): (u32, u32));
}

ffi! {
    #![unsafe(export("C"))]

    #[id(u32)]
    type OpaqueType<T>;

    #[erased(<u32>)]
    impl<T> Drop for dyn OpaqueType<T> {
        fn drop(&mut self);
    }

    #[erased]
    impl<dyn(u32) T> Clone for OpaqueType<T> {
        fn clone(&self);
    }
}

ffi! {
    #![unsafe(export("C"))]

    #[id(u32)]
    type OpaqueType<T>;

    #[erased(<u32>)]
    impl<T> Drop for dyn OpaqueType<T> {
        fn drop(&mut self);
    }

    #[erased(<u32, u8>)]
    impl<dyn(u32) T> Clone for OpaqueType<T> {
        fn clone(&self);
    }
}

fn main() {}
