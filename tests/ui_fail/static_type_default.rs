use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    type Declared<T>;

    impl<T> Drop for Declared<T> {
        fn drop(&mut self);
    }

    impl<T = u8> Declared<T>
    where
        use<T> @ <u8>,
    {
        #[symbol_name = "method_{U}"]
        fn method<U = u8>(&self, value: U)
        where
            use<U> @ <u8>;
    }

    #[symbol_name = "function_{T}"]
    fn function<T = u8>(value: T)
    where
        use<T> @ <u8>;
}

fn main() {}
