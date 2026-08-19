use co3::ffi;

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

ffi! {
    #![unsafe(export("C"))]

    #[unsafe(id(u8 = 0))]
    type Exported2<T>;

    impl<T> Drop for dyn Exported2<T>
    where
        use<T> @ <u32>,
    {
        fn drop(&mut self);
    }

    impl<dyn(u8) T: Unimplemented> RefKita for T
    where
        i32: Unimplemented,
        use<T> @ <Exported2<u32>>,
    {
        fn kita(&self) -> u32;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u64 = 1))]
    type Externed2<T>;

    impl<T> Drop for dyn Externed2<T>
    where
        use<T> @ <u32>,
    {
        #[symbol_name = "drop"]
        fn drop(&mut self, self_id: <dyn Externed2<T>>::ID);
    }

    impl<dyn(u64) T: Unimplemented> RefKita for T
    where
        i32: Unimplemented,
        use<T> @ <Externed2<u32>>,
    {
        #[symbol_name = "kita"]
        fn kita(&self, self_id: <dyn Self>::ID) -> u32;
    }
}

fn main() {}
