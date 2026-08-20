use co3::ffi;

ffi! {
    #![unsafe(export("C"))]

    impl<'a, T, const N: usize> Host {
        fn method<'a, T, const N: usize>();
    }
}

fn main() {}
