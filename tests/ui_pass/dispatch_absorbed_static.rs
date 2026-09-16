use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u8, unsafe(1))]
    type Resource<T>;

    impl<T> Drop for Resource<T> {
        fn drop(&mut self);
    }

    #[symbol_name = "inspect_resource"]
    fn inspect<T, dyn(u8) H>(resource: &H)
    where
        use<T> @ (<u8> | <u16>),
        use<H> @ <Resource<T>>;
}

fn main() {}
