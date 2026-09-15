use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    type Slot<T>;
    type Borrowed<'a>;

    impl<T> Drop for Slot<T> {
        fn drop(&mut self);
    }

    impl<'a> Drop for Borrowed<'a> {
        fn drop(&mut self);
    }
}

fn shorten<'short>(slot: &'short Slot<&'static u8>, _: &'short u8) -> &'short Slot<&'short u8> {
    slot
}

fn main() {}
