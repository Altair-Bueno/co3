use co3::{ffi};

trait Kita {
    type U;

    fn kita(self);
}

ffi! {}

ffi! {
    #![unsafe(extern("C"))]
    #![unsafe(export("C"))]
}

ffi! {
    #![unsafe(extern("Rust"))]
    #![unsafe(export("C"))]
}

ffi! {
    #![feature(generic_const_exprs)]

    #![unsafe(extern("C"))]
}

ffi! {
    #![feature(extern_types)]
    #![feature(extern_types)]

    #![unsafe(extern("C"))]
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
            <T> @ <>;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    impl<dyn(u32) U, dyn(u8) T> Kita for (T, U)
    where
        <U, T> @ <u32>,
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

    fn kita1((a, b): (u32, u32));
}

ffi! {
    #![unsafe(extern("C"))]

    #[id(u8)]
    type Handle<T>;

    impl<T> Drop for dyn Handle<T>
    where
        <T> @ <u32>,
    {
        fn drop(self_id: <dyn Self>::ID, &mut self);
    }

    impl<dyn(u8) T> Clone for Handle<T>
    where
        <T> @ <>,
    {
        fn clone(self_id: <dyn T>::ID, &self) -> Self;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[id(u8)]
    type Handle<T>;

    impl<T> Drop for dyn Handle<T>
    where
        <T> @ <>,
    {
        fn drop(self_id: <dyn Self>::ID, &mut self);
    }

    impl<dyn(u8) T> Clone for Handle<T>
    where
        <T> @ <u8, i8>,
    {
        fn clone(self_id: <dyn T>::ID, &self) -> Self;
    }
}

fn main() {}
