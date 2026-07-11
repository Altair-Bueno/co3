use co3::{ffi};

trait Kita {
    type U;

    fn kita(self);
}

ffi! {}

ffi! {
    #![extern("C")]
    #![export("C")]
}

ffi! {
    #![extern("Rust")]
    #![export("C")]
}

ffi! {
    #![feature(generic_const_exprs)]

    #![extern("C")]
}

ffi! {
    #![feature(extern_types)]
    #![feature(extern_types)]

    #![extern("C")]
}

ffi! {
    #![extern("C")]

    trait Kita {
        fn kita(self);
    }
}

ffi! {
    #![extern("C")]

    enum Kita {}
}

ffi! {
    #![extern("C")]

    struct Kita {}
}

ffi! {
    #![extern("C")]

    union Kita {}
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    impl Kita for u32 {
        #[dispatch]
        fn kita(self);
    }
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    #[dispatch(<u32>)]
    impl<dyn(u32) U, dyn(u8) T> Kita for (T, U) {
        fn kita(self, t_id: <dyn T>::ID, u_id: <dyn U>::ID);
    }
}

ffi! {
    #![extern("C")]

    #[dispatch]
    impl Kita {
        fn kita() {}
    }
}

ffi! {
    #![extern("C")]

    fn kita1(a: u32) {}
}

ffi! {
    #![extern("C")]

    fn kita1((a, b): (u32, u32));
}

ffi! {
    #![extern("C")]

    #[id(u8)]
    type Handle<T>;

    #[dispatch(<u32>)]
    impl<T> Drop for dyn Handle<T> {
        fn drop(self_id: <dyn Self>::ID, &mut self);
    }

    #[dispatch]
    impl<dyn(u8) T> Clone for Handle<T> {
        fn clone(self_id: <dyn T>::ID, &self) -> Self;
    }
}

ffi! {
    #![extern("C")]

    #[id(u8)]
    type Handle<T>;

    #[dispatch]
    impl<T> Drop for dyn Handle<T> {
        fn drop(self_id: <dyn Self>::ID, &mut self);
    }

    #[dispatch(<u8, i8>)]
    impl<dyn(u8) T> Clone for Handle<T> {
        fn clone(self_id: <dyn T>::ID, &self) -> Self;
    }
}

fn main() {}
