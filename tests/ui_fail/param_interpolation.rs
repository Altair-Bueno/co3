use co3::ffi;

ffi! {
    #![unsafe(export("C"))]

    impl<C> Unknown<C>
    where
        use<C> @ <u32>
    {
        #[symbol_name = "unknown__{X}"]
        fn unknown<D>()
        where
            use<D> @ <u8>;
    }

    #[symbol_name = "unknown__{X}"]
    fn unknown<C>()
    where
        use<C> @ <u8>;
}

ffi! {
    #![unsafe(extern("C"))]

    impl<C> Unknown<C>
    where
        use<C> @ <u32>
    {
        #[symbol_name = "unknown__{X}"]
        fn unknown<D>()
        where
            use<D> @ <u8>;
    }

    #[symbol_name = "unknown__{X}"]
    fn unknown<C>()
    where
        use<C> @ <u8>;
}

ffi! {
    #![unsafe(export("C"))]

    impl<dyn(u32) C> Runtime<C>
    where
        use<C> @ <u32>
    {
        #[symbol_name = "missing_static__{C}__{D}"]
        fn runtime<dyn(u8) D>()
        where
            use<D> @ <u8>;
    }

    #[symbol_name = "missing_static__{C}"]
    fn runtime<dyn(u8) C>()
    where
        use<C> @ <u8>;
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn(u8) C> Runtime<C>
    where
        use<C> @ <u32>
    {
        #[symbol_name = "missing_static__{C}__{D}"]
        fn runtime<dyn(u8) D>()
        where
            use<D> @ <u8>;
    }

    #[symbol_name = "missing_static__{C}"]
    fn runtime<dyn(u8) C>()
    where
        use<C> @ <u8>;
}

ffi! {
    #![unsafe(export("C"))]

    impl<C> Duplicate<C>
    where
        use<C> @ <u32>
    {
        #[symbol_name = "duplicate__{D}{D}{C}{C}"]
        fn duplicate<D>()
        where
            use<D> @ <u8>;
    }

    #[symbol_name = "duplicate__{C}{C}"]
    fn duplicate<C>()
    where
        use<C> @ <u8>;
}

ffi! {
    #![unsafe(extern("C"))]

    impl<C> Duplicate<C>
    where
        use<C> @ <u32>
    {
        #[symbol_name = "duplicate__{D}{D}{C}{C}"]
        fn duplicate<D>()
        where
            use<D> @ <u8>;
    }

    #[symbol_name = "duplicate__{C}{C}"]
    fn duplicate<C>()
    where
        use<C> @ <u8>;
}

ffi! {
    #![unsafe(export("C"))]

    #[unsafe(id(u8))]
    type ImplGeneric<T>;

    impl<T> Drop for dyn ImplGeneric<T>
    where
        use<T> @ <u8>
    {
        #[symbol_name = "impl_drop{T}"]
        fn drop(&mut self);
    }

    #[symbol_name = "erased__{C}"]
    fn erased<C>()
    where
        use<C> @ <u8>;
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u8))]
    type ImplGeneric<T>;

    impl<T> Drop for dyn ImplGeneric<T>
    where
        use<T> @ <u8>
    {
        #[symbol_name = "impl_drop{T}"]
        fn drop(&mut self);
    }

    #[symbol_name = "erased__{C}"]
    fn erased<C>()
    where
        use<C> @ <u8>;
}

fn main() {}
