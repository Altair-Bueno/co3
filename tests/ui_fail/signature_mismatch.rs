use co3::{ffi, handles};

trait Kita {
    type MySelf;
    fn kita(&self) -> Vec<Self::MySelf>;
    fn kita2(a: &u32);
}

impl Kita for core::ffi::c_void {
    type MySelf = usize;

    fn kita(&self) -> Vec<Self::MySelf> {
        unreachable!()
    }

    fn kita2(_: &u32) {
        unreachable!()
    }
}

handles! {
    unsafe {
        Opaque1,
        Opaque2,
    }
}

impl Kita for u32 {
    type MySelf = Self;

    fn kita(&self) -> Vec<Self::MySelf> {
        unimplemented!()
    }

    fn kita2(_a: &u32) {}
}

fn kita(_a: &u32) {}

ffi! {
    #![unsafe(export("C"))]

    fn kita(a: &Box<u32>);
}

ffi! {
    #![unsafe(export("C"))]

    impl Kita for u32 {
        fn kita2(a: &Box<u32>);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    #[unsafe(id(u32))]
    type Opaque1;
    #[unsafe(id(u8))]
    type Opaque2;

    impl ToOwned for Opaque1 {
        type Owned = OwnedOpaque1;

        move fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }

    impl ToOwned for Opaque2 {
        type Owned = OwnedOpaque2;

        move fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }

    impl<dyn(u32) T: ToOwned> Kita for T
    where
        use<T> @ (<Opaque1> | <Opaque2Alias>),
    {
        type MySelf = <T as ToOwned>::Owned;

        move fn kita(self_id: <dyn Self>::ID, self: &Self) -> Vec<<Self as Kita>::MySelf>;
        fn kita2(a: &u32, self_id: <dyn T>::ID);
    }
}

type Opaque2Alias = Opaque2;

fn main() {}
