use co3::{ffi, handles};

handles! { unsafe {
    OpaqueSlice<u8>,
}}

struct OpaqueSlice<T>([T]);

ffi! {
    #![unsafe(export("C"))]

    #[id(u8)]
    type OpaqueSlice<T>;

    #[erased(<u8>)]
    impl<T> Drop for dyn OpaqueSlice<T> {
        fn drop(&mut self);
    }
}

fn main() {}
