use co3::ffi;

trait Dispatch {
    fn dispatch(&self);
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn(u8) T> Dispatch for T
    where
        <T> @ <u32> | <u64>,
    {
        fn dispatch(&self);
    }
}

fn main() {}
