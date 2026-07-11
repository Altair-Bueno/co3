use core::{borrow::Borrow, ffi::c_void};

use co3::{ReprC, ffi, handles};

#[derive(ReprC)]
#[reprC(id(u8))]
#[repr(transparent)]
struct Unsized<T: ?Sized>(T);

#[derive(ReprC)]
#[repr(C)]
struct Wrapper<T: ?Sized>(Box<T>);

handles! {
    Unsized<str>,
}

impl From<Box<Unsized<str>>> for Unsized<String> {
    fn from(_value: Box<Unsized<str>>) -> Self {
        unimplemented!()
    }
}

impl Borrow<Unsized<str>> for Unsized<String> {
    fn borrow(&self) -> &Unsized<str> {
        unimplemented!()
    }
}

impl ToOwned for Unsized<str> {
    type Owned = Unsized<String>;

    fn to_owned(&self) -> Self::Owned {
        unimplemented!()
    }
}

impl Clone for Wrapper<Unsized<str>> {
    fn clone(&self) -> Self {
        unimplemented!()
    }
}

mod provider {
    use co3::ffi;

    use super::*;

    impl From<Unsized<String>> for Box<Unsized<str>> {
        fn from(_: Unsized<String>) -> Self {
            unimplemented!()
        }
    }

    impl Wrapper<Unsized<str>> {
        fn take_export(self) -> usize {
            unimplemented!()
        }
    }

    ffi! {
        #![export("C")]

        #[dispatch(<Unsized<str>>)]
        impl<dyn(u8) T = [c_void]> Wrapper<T> {
            fn take_export(self) -> usize;
        }
    }
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    #[dispatch(<Unsized<str>>)]
    impl<dyn(u8) T = [c_void]> Wrapper<T> {
        fn take(self, handle_id: <dyn T>::ID) -> usize;
    }
}

fn main() {}
