use co3::ffi;

trait RefKita {
    fn kita(&self) -> u32;
}

struct Exported0(u8);
impl RefKita for Exported0 {
    fn kita(&self) -> u32 {
        0
    }
}

struct Exported1(u8);
impl RefKita for Exported1 {
    fn kita(&self) -> u32 {
        0
    }
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "dispatch_char_export"]

    #[unsafe(id(char = 0 as char))]
    type Exported0;

    impl<dyn(char) T> RefKita for T
    where
        use<T> @ <Exported0>,
    {
        fn kita(&self) -> u32;
    }
}

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "kita"]

    #[unsafe(id(char = 1 as char))]
    type Externed0;

    impl<dyn(char) T> RefKita for T
    where
        use<T> @ <Externed0>,
    {
        #[symbol_name = "kita"]
        fn kita(&self, self_id: <dyn Self>::ID) -> u32;
    }
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "dispatch_u32_export"]

    #[unsafe(id(u32 = 0))]
    type Exported1;

    impl<dyn(u32) T> RefKita for T
    where
        use<T> @ (<Exported1> | <Exported1>),
    {
        fn kita(&self) -> u32;
    }
}

fn main() {}
