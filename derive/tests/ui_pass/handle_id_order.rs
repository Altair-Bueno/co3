use co3::{
    ffi,
    handle::{Handle, HandleFamily},
    handles,
};

handles! {
    unsafe {
        Opaque1,
        Opaque2,
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "this_crate"]

    #[id(u32)]
    type Opaque1;
    #[id(u8)]
    #[derive(PartialEq)]
    type Opaque2;

    impl ToOwned for Opaque1 {
        type Owned = OwnedOpaque1;

        #[symbol_name = "this_crate__ToOwned__Box_Opaque1__to_owned"]
        move fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }

    impl ToOwned for Opaque2 {
        type Owned = OwnedOpaque2;

        #[symbol_name = "this_crate__ToOwned__Box_Opaque2__to_owned"]
        move fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }

    impl Default for OwnedOpaque1 {
        #[symbol_name = "this_crate__Default__Box_Opaque1__default"]
        move fn default() -> Self;
    }

    impl Default for OwnedOpaque2 {
        #[symbol_name = "this_crate__Default__Box_Opaque2__default"]
        move fn default() -> Self;
    }

    fn kita1(
        inc_id: <Opaque2 as HandleFamily>::Kind,
        a_id: <Opaque1 as HandleFamily>::Kind,
        a: &mut Opaque1,
        inc: &Opaque2,
    ) -> u8;
}

mod provider {
    use co3::{ffi, handles};

    trait Custom<T> {
        fn kita1(&mut self, inc: &T) -> u8;
    }

    #[derive(Clone)]
    pub struct Opaque1(u8);
    #[derive(Clone)]
    pub struct Opaque2(u8);

    handles! {
        unsafe {
            Opaque1,
            Opaque2,
        }
    }

    impl Default for Box<Opaque1> {
        fn default() -> Self {
            Box::new(Opaque1(0))
        }
    }

    impl Default for Opaque2 {
        fn default() -> Self {
            Self(0)
        }
    }

    impl<T> Custom<T> for Opaque1 {
        fn kita1(&mut self, _inc: &T) -> u8 {
            0
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #![symbol_prefix = "this_crate"]

        #[id(u32)]
        type Opaque1;
        #[id(u8)]
        type Opaque2;

        impl Default for Box<Opaque1> {
            #[symbol_name = "this_crate__Default__Box_Opaque1__default"]
            move fn default() -> Self;
        }

        impl Drop for Opaque2 {
            #[symbol_name = "this_crate__Drop__Opaque2__drop"]
            fn drop(&mut self);
        }

        impl Default for Box<Opaque2> {
            #[symbol_name = "this_crate__Default__Box_Opaque2__default"]
            move fn default() -> Self;
        }

        impl ToOwned for Box<Opaque1> {
            #[symbol_name = "this_crate__ToOwned__Box_Opaque1__to_owned"]
            move fn to_owned(&self) -> <Self as ToOwned>::Owned;
        }

        impl ToOwned for Box<Opaque2> {
            #[symbol_name = "this_crate__ToOwned__Box_Opaque2__to_owned"]
            move fn to_owned(&self) -> <Self as ToOwned>::Owned;
        }

        #[dispatch(<Opaque2, Opaque1>)]
        impl<dyn(u8) T, dyn(u32) U> Custom<T> for U {
            #[symbol_name = "this_crate__kita1"]
            fn kita1(&mut self, inc: &T) -> u8;
        }
    }
}

fn main() {
    let mut value1 = OwnedOpaque1::default();
    let value2 = OwnedOpaque2::default();

    let _ = kita1(Opaque2::ID, Opaque1::ID, &mut value1, &value2);
}
