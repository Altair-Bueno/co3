use co3::ffi;

unsafe impl co3::tag::Tagged for OpaqueSlice<u8> {
    const TAG: u8 = 0;
}

struct OpaqueSlice<T>([T]);

ffi! {
    #![unsafe(export("C"))]

    #[tag(u8)]
    type OpaqueSlice<T>;

    impl<T> Drop for dyn OpaqueSlice<T>
    where
        use<T> @ <u8>,
    {
        fn drop(&mut self);
    }
}

fn main() {}
