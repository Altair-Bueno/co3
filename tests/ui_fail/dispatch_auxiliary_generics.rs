use co3::ffi;

struct Wrapper<T>(T);
struct Host;

fn leaked<T, U>(_: &U) {}

ffi! {
    #![unsafe(export("C"))]

    fn leaked<dyn(u8) T, U>(value: &U)
    where
        use<T> @ (<u32>);
}

ffi! {
    #![unsafe(extern("C"))]

    fn missing_use<dyn(u8) T>();
}

ffi! {
    #![unsafe(extern("C"))]

    fn payload<dyn(u8) T = u32, U>()
    where
        use<T> @ (<Wrapper<U>>);
}

ffi! {
    #![unsafe(extern("C"))]

    fn partial<dyn(u8) T, U>()
    where
        use<T> @ (<Wrapper<U>> | <u32>);
}

ffi! {
    #![unsafe(extern("C"))]

    fn unanchored<dyn(u8) T, V>()
    where
        use<T> @ (<u32>),
        use<V> @ (<u8>);
}

ffi! {
    #![unsafe(extern("C"))]

    impl Host {
        fn unanchored_method<dyn(u8) T, V>(&self)
        where
            use<T> @ (<u32>),
            use<V> @ (<u8>);
    }
}

fn main() {}
