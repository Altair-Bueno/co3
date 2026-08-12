use co3::{ffi, handles};

trait Kita {}

struct Export1<T>(T);

impl<T> Kita for Export1<T> {}

handles! {
    unsafe {
        Export1<u32>,
    }
}

ffi! {
    #![unsafe(export("C"))]

    #[id(u8)]
    type OpaqueType<T>;
}

ffi! {
    #![unsafe(extern("C"))]

    #[id(u8)]
    type ExternType<T>;
}

ffi! {
    #![unsafe(export("C"))]

    #![symbol_prefix = "kita"]

    impl Drop for OpaqueType {
        fn drop(&mut self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    impl Drop for ExternType {
        fn drop(&mut self);
    }
}

ffi! {
    #![unsafe(export("C"))]

    #[id(u32)]
    type Export1<T>;

    // TODO: These Drop impls could be allowed
    impl<T> Drop for dyn Export1<T>
    where
        Self: Kita,
        use<T> @ <u32>,
    {
        fn drop(&mut self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    #[id(u32)]
    type Extern2<T>;

    impl<T> Drop for dyn Extern2<T>
    where
        Self: Kita,
        use<T> @ (<u32>),
    {
        fn drop(&mut self, self_id: <dyn Self>::ID);
    }
}

fn main() {}
