use co3::{ffi, handles};

pub trait Unimplemented {}

trait RefKita {
    fn kita(&self) -> u32;
}

struct Exported2<T>(T);
impl RefKita for Exported2<u32> {
    fn kita(&self) -> u32 {
        unimplemented!()
    }
}

handles! {
    unsafe {
        Exported2<u32>,
        Externed2<u32>,
    }
}

ffi! {
    #![unsafe(export("C"))]

    #[id(u8)]
    type Exported2<T>;

    #[erased(<u32>)]
    impl<T> Drop for dyn Exported2<T> {
        fn drop(&mut self);
    }

    #[erased(<Exported2<u32>>)]
    impl<dyn(u8) T: Unimplemented> RefKita for T where i32: Unimplemented {
        fn kita(&self) -> u32;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[id(u64)]
    type Externed2<T>;

    #[erased]
    impl<T> Drop for dyn Externed2<T> {
        #[symbol_name = "drop"]
        fn drop(&mut self, self_id: <dyn Externed2<T>>::ID);
    }

    #[erased(<Externed2<u32>>)]
    impl<dyn(u64) T: Unimplemented> RefKita for T where i32: Unimplemented {
        #[symbol_name = "kita"]
        fn kita(&self, self_id: <dyn Self>::ID) -> u32;
    }
}

fn main() {}
