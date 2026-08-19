use co3::{ffi, handle::Handle};

unsafe impl Handle for OpaqueSlice<u8> {
    const ID: u8 = 0;
}

struct OpaqueSlice<T>([T]);

ffi! {
    #![unsafe(export("C"))]

    #[unsafe(id(u8))]
    type OpaqueSlice<T>;

    impl<T> Drop for dyn OpaqueSlice<T>
    where
        use<T> @ <u8>,
    {
        fn drop(&mut self);
    }
}

fn main() {}
