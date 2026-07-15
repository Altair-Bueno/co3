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
        #[dispatch]
        fn kita(self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    #[dispatch(<u32>)]
    impl<dyn(u32) U, dyn(u8) T> Kita for (T, U) {
        fn kita(self, t_id: <dyn T>::ID, u_id: <dyn U>::ID);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[dispatch]
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
    #![unsafe(extern("C"))]

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
