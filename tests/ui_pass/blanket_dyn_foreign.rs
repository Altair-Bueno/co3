use co3::ffi;

trait Describe {
    fn describe(&self) -> u8;
}

mod provider {
    use super::*;

    struct First(u8);
    struct Second(u8);

    impl First {
        fn value(&self) -> u8 {
            1
        }
    }

    impl Second {
        fn value(&self) -> u8 {
            2
        }
    }

    impl Describe for First {
        fn describe(&self) -> u8 {
            1
        }
    }

    impl Describe for Second {
        fn describe(&self) -> u8 {
            2
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #[unsafe(id(u8 = 1))]
        type First;

        #[unsafe(id(u8 = 2))]
        type Second;

        impl<T> dyn T
        where
            use<T> @ (<First> | <Second>),
        {
            #[symbol_name = "blanket_dyn_value"]
            fn value(&self) -> u8;
        }

        impl<T> Describe for dyn T
        where
            use<T> @ (<First> | <Second>),
        {
            #[symbol_name = "blanket_dyn_describe"]
            fn describe(&self) -> u8;
        }

        impl<T> Drop for dyn T
        where
            use<T> @ (<First> | <Second>),
        {
            #[symbol_name = "blanket_dyn_drop"]
            fn drop(&mut self);
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u8 = 1))]
    type First;

    #[unsafe(id(u8 = 2))]
    type Second;

    impl<T> dyn T
    where
        use<T> @ (<First> | <Second>),
    {
        #[symbol_name = "blanket_dyn_value"]
        fn value(&self) -> u8;
    }

    impl<T> Describe for dyn T
    where
        use<T> @ (<First> | <Second>),
    {
        #[symbol_name = "blanket_dyn_describe"]
        fn describe(&self) -> u8;
    }

    impl<T> Drop for dyn T
    where
        use<T> @ (<First> | <Second>),
    {
        #[symbol_name = "blanket_dyn_drop"]
        fn drop(&mut self);
    }
}

fn main() {}
