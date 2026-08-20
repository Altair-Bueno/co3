use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    type Kita<T>;

    impl<dyn(u8) T> Drop for Kita<T>
    where
        use<T> @ <Self>
    {
        fn drop(&mut self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    fn cycle<T, U>()
    where
        use<T> @ <U>,
        use<U> @ <T>;
}

fn main() {}
