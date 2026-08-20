use co3::ffi;

ffi! {
    #![unsafe(export("C"))]

    impl<'a, T, dyn(u8) V, const N: usize> Unused {
        fn unused<'b, U, dyn(u8) W, const K: usize>();
    }

    fn unused<'a, T, dyn(u8) V, const N: usize>();
}

ffi! {
    #![unsafe(extern("C"))]

    impl<'a, T, dyn(u8) V, const N: usize> Unused {
        fn unused<'b, U, dyn(u8) W, const K: usize>();
    }

    fn unused<'a, T, dyn(u8) V, const N: usize>();
}

ffi! {
    #![unsafe(export("C"))]

    impl<T, dyn(u8) V, const N: usize> Bounded
    where
        use<N, T, V> @ <12, u32, u8>,
    {
        fn unused<U, dyn(u8) W, const K: usize>()
        where
            use<K, U, W> @ <12, u32, u8>;
    }

    fn unused<T, dyn(u8) V, const N: usize>()
    where
        use<N, T, V> @ <12, u32, u8>;
}

ffi! {
    #![unsafe(extern("C"))]

    impl<T, dyn(u8) V, const N: usize> Bounded
    where
        use<N, T, V> @ <12, u32, u8>,
    {
        fn unused<U, dyn(u8) W, const K: usize>()
        where
            use<K, U, W> @ <12, u32, u8>;
    }

    fn unused<T, dyn(u8) V, const N: usize>()
    where
        use<N, T, V> @ <12, u32, u8>;
}

ffi! {
    #![unsafe(export("C"))]

    impl<'a, T, dyn(u8) V, const N: usize> Used {
        fn used(value: &'a [(T, V); N]);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl<'a, T, dyn(u8) V, const N: usize> Used {
        fn used(value: &'a [(T, V); N]);
    }
}

ffi! {
    #![unsafe(export("C"))]

    impl<T, dyn(u8) V> Used<V>
    where
        use<V> @ (<T> | <u32>)
    {
        fn used<U, dyn(u8) W>(value: &W)
        where
            use<W> @ (<u32> | <U>);
    }

    fn used<U, dyn(u8) W>(value: &W)
    where
        use<W> @ (<u32> | <U>);
}

ffi! {
    #![unsafe(extern("C"))]

    impl<T, dyn(u8) V> Used<V>
    where
        use<V> @ (<T> | <u32>)
    {
        fn used<U, dyn(u8) W>(value: &W)
        where
            use<W> @ (<u32> | <U>);
    }

    fn used<U, dyn(u8) W>(value: &W)
    where
        use<W> @ (<u32> | <U>);
}

fn main() {}
