use co3::{ffi, handles};

handles! { unsafe {
    OpaqueSlice<u8>,
}}

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
