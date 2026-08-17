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

    #[unsafe(id(u8))]
    type OpaqueType<T>;
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u8))]
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

    #[unsafe(id(u32))]
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

    #[unsafe(id(u32))]
    type Extern2<T>;

    impl<T> Drop for dyn Extern2<T>
    where
        Self: Kita,
        use<T> @ (<u32>),
    {
        fn drop(&mut self, self_id: <dyn Self>::ID);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u8))]
    type IncompleteDispatch<T, U>;

    impl<T, U> Drop for dyn IncompleteDispatch<T, U>
    where
        use<T> @ (<u8>)
    {
        fn drop(&mut self);
    }
}

fn main() {}
