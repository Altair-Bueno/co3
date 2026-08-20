use co3::ffi;

trait Kita {
    type U;

    fn kita(self);
}

ffi! {}

ffi! {
    #![unsafe(extern("C"))]
    #![unsafe(extern("C"))]
}

ffi! {
    #![unsafe(extern("Rust"))]
    #![unsafe(extern("C"))]
}

ffi! {
    #![feature(generic_const_exprs)]

    #![unsafe(extern("C"))]
}

ffi! {
    #![unsafe(extern("C"))]

    #![feature(extern_types)]
    #![feature(extern_types)]
}

ffi! {
    #![unsafe(extern("C"))]

    trait Kita {
        fn kita(self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    enum Kita {}
}

ffi! {
    #![unsafe(extern("C"))]

    struct Kita {}
}

ffi! {
    #![unsafe(extern("C"))]

    union Kita {}
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    impl Kita for u32 {
        fn kita<T>(self)
        where
            use<T> @ <u32>;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    impl<dyn(u32) U, dyn(u8) T> Kita for (T, U)
    where
        use<U, T> @ <u32>,
    {
        fn kita(self, t_id: <dyn T>::ID, u_id: <dyn U>::ID);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl Kita {
        fn kita() {}
    }
}

ffi! {
    #![unsafe(extern("C"))]

    fn kita1(a: u32) {}
}

ffi! {
    #![unsafe(extern("C"))]

    fn invalid_tag_target<dyn(u8) T = u8>(tag: <dyn u32>::ID)
    where
        use<T> @ <u32>;
}

ffi! {
    #![unsafe(extern("C"))]

    fn duplicate_tag<dyn(u8) T = u8>(first: <dyn T>::ID, second: <dyn T>::ID)
    where
        use<T> @ <u32>;
}

ffi! {
    #![unsafe(extern("C"))]

    fn kita1((a, b): (u32, u32));
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u8))]
    type Handle<T>;

    impl<T> Drop for dyn Handle<T>
    where
        use<T> @ <u32>,
    {
        fn drop(self_id: <dyn Self>::ID, &mut self);
    }

    impl<dyn(u8) T> Clone for Handle<T>
    where
        use<T> @ <>,
    {
        fn clone(self_id: <dyn T>::ID, &self) -> Self;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(i32))]
    type GenericType<T>;

    impl<T> GenericType<T> {
        fn method(&self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u8))]
    type Handle<T>;

    impl<dyn(u8) T> Clone for Handle<T>
    where
        use<T> @ <u8, i8>,
    {
        fn clone(self_id: <dyn T>::ID, &self) -> Self;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u32))]
    type OpaqueType<T>
    where
        use<T> @ <u32>;

    impl<T> Drop for dyn OpaqueType<T>
    where
        use<T> @ <u32>,
    {
        fn drop(self_id: <dyn Self>::ID, &mut self);
    }
}

fn main() {}
