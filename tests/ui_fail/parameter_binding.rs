use co3::ffi;

ffi! {
    #![unsafe(export("C"))]

    impl<T, dyn(u8) U, const N: usize> Exposed<T, N, U> {
        fn exposed<L, dyn(u8) V, const K: usize>(value: (L, &V), len: K);
    }

    fn exposed<T, dyn(u8) L, const N: usize>(value: (T, &L), len: N);
}

ffi! {
    #![unsafe(extern("C"))]

    impl<T, dyn(u8) U, const N: usize> Exposed<T, N, U> {
        fn exposed<L, dyn(u8) V, const K: usize>(value: (L, &V), len: K);
    }

    fn exposed<T, dyn(u8) U, const N: usize>(value: (T, &U), len: N);
}

ffi! {
    #![unsafe(export("C"))]

    impl<T, dyn(u8) U> Bounded<U, T>
    where
        use<T> @ <u8>,
        use<U> @ <u8>,
    {
        fn rebound()
        where
            use<T, U> @ (<u16, u8>);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl<T, dyn(u8) U> Bounded<U, T>
    where
        use<T> @ <u8>,
        use<U> @ <u8>,
    {
        fn rebound()
        where
            use<T, U> @ (<u16, u8>);
    }
}

ffi! {
    #![unsafe(export("C"))]

    impl<T> Bounded<T>
    where
        use<T> @ <u8>,
        use<T> @ <i8>,
    {
        fn rebound<U>()
        where
            use<U> @ <u8>,
            use<U> @ <i8>;
    }

    fn rebound<U>()
    where
        use<U> @ <u8>,
        use<U> @ <i8>;
}

ffi! {
    #![unsafe(extern("C"))]

    impl<T> Bounded<T>
    where
        use<T> @ <u8>,
        use<T> @ <i8>,
    {
        fn rebound<U>()
        where
            use<U> @ <u8>,
            use<U> @ <i8>;
    }

    fn rebound<U>()
    where
        use<U> @ <u8>,
        use<U> @ <i8>;
}

ffi! {
    #![unsafe(export("C"))]

    impl<dyn(u8) T = u32, U> BehindNonErased<T>
    where
        use<T> @ <U>,
    {
        fn behind_non_erased<dyn(u8) K = u32, L>()
        where
            use<K> @ <L>;
    }

    fn behind_non_erased<dyn(u8) T = u32, U>()
    where
        use<T> @ (<U>);
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn(u8) T = u32, U> BehindNonErased<T>
    where
        use<T> @ <U>,
    {
        fn behind_non_erased<dyn(u8) K = u32, L>()
        where
            use<K> @ <L>;
    }

    fn behind_non_erased<dyn(u8) T = u32, U>()
    where
        use<T> @ (<U>);
}

ffi! {
    #![unsafe(export("C"))]

    impl<dyn(u8) T, U> BehindErased<T>
    where
        use<T> @ (<U> | <u32>),
    {
        fn behind_erased<dyn(u8) K, L>()
        where
            use<K> @ (<L> | <u32>);
    }

    fn behind_erased<dyn(u8) T, U>()
    where
        use<T> @ (<U> | <u32>);
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn(u8) T, U> BehindErased<T>
    where
        use<T> @ (<U> | <u32>),
    {
        fn behind_erased<dyn(u8) K, L>()
        where
            use<K> @ (<L> | <u32>);
    }

    fn behind_erased<dyn(u8) T, U>()
    where
        use<T> @ (<U> | <u32>);
}

ffi! {
    #![unsafe(export("C"))]

    impl<dyn(u8) T, dyn(u8) U> BehindErased<T>
    where
        use<T> @ <U>,
    {
        fn dynamic_auxiliary<dyn(u8) K, dyn(u8) L>()
        where
            use<K> @ <L>;
    }

    fn dynamic_auxiliary<dyn(u8) T, dyn(u8) A>()
    where
        use<T> @ (<A>);
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn(u8) T, dyn(u8) U> BehindErased<T>
    where
        use<T> @ <U>,
    {
        fn dynamic_auxiliary<dyn(u8) K, dyn(u8) L>()
        where
            use<K> @ <L>;
    }

    fn dynamic_auxiliary<dyn(u8) T, dyn(u8) A>()
    where
        use<T> @ (<A>);
}

ffi! {
    #![unsafe(export("C"))]

    impl<T, U> BehindStatic<T>
    where
        use<T> @ <U>,
    {
        fn behind_static<K, L>()
        where
            use<K> @ <L>;
    }

    fn behind_static<T, U>()
    where
        use<T> @ (<U>);
}

ffi! {
    #![unsafe(extern("C"))]

    impl<T, U> BehindStatic<T>
    where
        use<T> @ <U>,
    {
        fn behind_static<K, L>()
        where
            use<K> @ <L>;
    }

    fn behind_static<T, U>()
    where
        use<T> @ (<U>);
}

ffi! {
    #![unsafe(export("C"))]

    #[tag(u8)]
    type Unconstrained<T>;

    impl<T> Drop for dyn Unconstrained<T>
    where
        use<T> @ <u32>,
    {
        fn drop(&mut self);
    }

    impl<T> Unconstrained<T> {
        fn unconstrained<U, dyn(u8) K>()
        where
            use<K> @ <U>;
    }

    fn unconstrained<U, dyn(u8) K>()
    where
        use<K> @ <U>;
}

fn main() {}
