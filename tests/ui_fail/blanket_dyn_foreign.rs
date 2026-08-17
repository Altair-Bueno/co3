use co3::ffi;

trait Trait {
    fn method(&self);
}

ffi! {
    #![unsafe(extern("C"))]

    #[id(u8)]
    type First;

    #[id(u16)]
    type DifferentTag;

    impl<T> Trait for dyn T
    where
        use<T> @ (<First> | <DifferentTag>),
    {
        fn method(&self);
    }
}

struct Local;

ffi! {
    #![unsafe(extern("C"))]

    #[id(u8)]
    type Declared;

    impl<T> dyn T
    where
        use<T> @ (<Declared> | <Local>),
    {
        fn method(&self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    type MissingTag;

    impl<T> dyn T
    where
        use<T> @ (<MissingTag>),
    {
        fn method(&self);
    }
}

fn main() {}
