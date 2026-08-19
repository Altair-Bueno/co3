use co3::ffi;

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
        fn kita(self)
        where
            use<T> @ <>;
    }
}

ffi! {
    #![unsafe(export("C"))]

    type OpaqueType<T>
    where
        use<T> @ <>;
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

    #[unsafe(id(u32))]
    type OpaqueType<T>;

    impl<T> Drop for dyn OpaqueType<T>
    where
        use<T> @ <u32>,
    {
        fn drop(&mut self);
    }

    impl<dyn(u32) T> Clone for OpaqueType<T>
    where
        use<T> @ <>
    {
        fn clone(&self);
    }
}

ffi! {
    #![unsafe(export("C"))]

    #[unsafe(id(u32))]
    type OpaqueType<T>;

    impl<T> Drop for dyn OpaqueType<T>
    where
        use<T> @ <u32>,
    {
        fn drop(&mut self);
    }

    impl<dyn(u32) T> Clone for OpaqueType<T>
    where
        use<T> @ <u32, u8>,
    {
        fn clone(&self);
    }
}

ffi! {
    #![unsafe(export("C"))]

    type GenericExport<T>;

    impl<T> GenericExport<T> {
        fn method(&self);
    }
}

struct ReceiverPosition;

impl ReceiverPosition {
    fn method(&self, _: u32) {}
}

ffi! {
    #![unsafe(export("C"))]

    type ReceiverPosition;

    impl ReceiverPosition {
        fn method(value: u32, &self);
    }
}

fn main() {}
