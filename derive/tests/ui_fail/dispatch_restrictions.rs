use co3::{export_C, extern_C, handles, handle::Handle};

pub trait Unimplemented {}

trait Kita {
    fn kita(self) -> u32;
}

trait RefKita {
    fn kita(&self) -> u32;
}

struct Exported0;
impl Kita for Exported0 {
    fn kita(self) -> u32 {
        unimplemented!()
    }
}
impl RefKita for Exported0 {
    fn kita(&self) -> u32 {
        unimplemented!()
    }
}

struct Exported1;
impl Kita for Exported1 {
    fn kita(self) -> u32 {
        unimplemented!()
    }
}
impl RefKita for Exported1 {
    fn kita(&self) -> u32 {
        unimplemented!()
    }
}

struct Exported2<T>(T);
impl Kita for Exported2<u32> {
    fn kita(self) -> u32 {
        unimplemented!()
    }
}
impl RefKita for Exported2<u32> {
    fn kita(&self) -> u32 {
        unimplemented!()
    }
}

unsafe impl Handle for Exported0 {
    const ID: char = 0 as char;
}

unsafe impl Handle for Externed0 {
    const ID: char = 1 as char;
}

handles! {
    Exported1,
    Externed1,
    Exported2<u32>,
    Externed2<u32>
}

export_C! {
    #[dispatch(<u32>)]
    impl<dyn(u8) T> Kita for T {
        fn kita(self, self_id: <dyn T>::ID) -> u32;
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<u32>)]
    impl<dyn(u32) T> Kita for T {
        #[link_name = "kita"]
        fn kita(&self) -> u32;
    }
}

export_C! {
    #[dispatch(<'a, u32>)]
    #[unsafe(lifetimes)]
    impl<'a, dyn(u8) T> Kita<'a> for T {
        fn drop(&mut self);
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[unsafe(lifetimes)]
    #[dispatch(<'a, u32>)]
    impl<'a, dyn(u8) T> Kita<'a> for T {
        fn drop(&mut self, self_id: <dyn Self>::ID);
    }
}

export_C! {
    #[dispatch(<&i16>, <&'_ i32>, <&'a u32>)]
    #[unsafe(lifetimes)]
    impl<'a, dyn(u8) T> Kita<'a> for T {
        fn kita(self);
    }
}

extern_C! {
    #[dispatch(<&i16>, <&'_ i32>, <&'a u32>)]
    #[unsafe(lifetimes)]
    impl<'a, dyn(u8) T> Kita<'a> for T {
        #[link_name = "kita"]
        fn kita(handle_id: <dyn T>::ID, self);
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for T {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for T {
        #[link_name = "kita"]
        fn kita(self) -> u32;
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<dyn T> Kita for T {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #[dispatch(<u32>)]
    impl<dyn T> Kita for T {
        #[link_name = "kita"]
        fn kita(self) -> u32;
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for dyn T {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for dyn T {
        #[link_name = "kita"]
        fn kita(self) -> u32;
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for dyn u32 {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<u32>)]
    impl<T> Kita for dyn u32 {
        #[link_name = "kita"]
        fn kita(&self, self_id: <dyn Self>::ID) -> u32;
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for dyn Option<T> {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for dyn Option<T> {
        #[link_name = "kita"]
        fn kita(&self, self_id: <dyn Self>::ID) -> u32;
    }
}

extern_C! {
    #[dispatch(<u32>)]
    impl<dyn(i64) T> Kita for T {
        #[link_name = "kita"]
        fn kita(self, self_id: <dyn Self>::ID) -> <dyn T>::ID;
    }
}

// TODO: This produces extra unrelated error message
extern_C! {
    #[dispatch(<u32>)]
    impl<dyn(u64) T> Kita for T {
        #[link_name = "kita"]
        fn kita(self, self_id: (<dyn Self>::ID,)) -> u32;
    }
}

export_C! {
    #[id(char)]
    type Exported0;

    #[dispatch(<Exported0>)]
    impl<dyn(char) T> RefKita for T {
        fn kita(&self) -> u32;
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[id(char)]
    type Externed0;

    #[dispatch(<Externed0>)]
    impl<dyn(char) T> RefKita for T {
        #[link_name = "kita"]
        fn kita(&self, self_id: <dyn Self>::ID) -> u32;
    }
}

export_C! {
    #[id(u32)]
    type Exported1;

    #[dispatch(<Exported1>, <Exported1>)]
    impl<dyn(u32) T> RefKita for T {
        fn kita(&self) -> u32;
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[id(u32)]
    type Externed1;

    #[dispatch(<Externed1>, <Externed1>)]
    impl<dyn(u32) T> RefKita for T {
        #[link_name = "kita"]
        fn kita(self_id: <dyn T>::ID, &self) -> u32;
    }
}

export_C! {
    #[id(u8)]
    type Exported2<T>;

    #[dispatch(<u32>)]
    impl<T> Drop for dyn Exported2<T> {
        fn drop(&mut self);
    }

    #[dispatch(<Exported2<u32>>)]
    impl<dyn(u8) T: Unimplemented> RefKita for T where i32: Unimplemented {
        fn kita(&self) -> u32;
    }
}

extern_C! {
    #[id(u64)]
    type Externed2<T>;

    #[dispatch]
    impl<T> Drop for dyn Externed2<T> {
        #[link_name = "drop"]
        fn drop(&mut self, self_id: <dyn Externed2<T>>::ID);
    }

    #[dispatch(<Externed2<u32>>)]
    impl<dyn(u64) T: Unimplemented> RefKita for T where i32: Unimplemented {
        #[link_name = "kita"]
        fn kita(&self, self_id: <dyn Self>::ID) -> u32;
    }
}

fn main() {}
