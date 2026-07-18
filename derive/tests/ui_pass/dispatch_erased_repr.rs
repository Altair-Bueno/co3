use co3::{rust_spec::TypeSpec, ReprC, ffi, handles};

trait Attribute {}

trait ByteValue {
    fn into_byte(self) -> u8;
    fn add_ref(&self, rhs: &Self) -> u8;
}

#[derive(TypeSpec, ReprC)]
#[repr(transparent)]
struct EnvAttr(usize);

#[derive(Clone, TypeSpec, ReprC)]
#[reprC(id(u16))]
struct Custom1(usize);

#[derive(TypeSpec, ReprC)]
#[reprC(id(u16))]
#[repr(transparent)]
struct Custom2<'a>(&'a u8);

handles! {
    unsafe {
        Custom1,
        Custom2<'_>,
    }
}

impl Attribute for Custom1 {}
impl Attribute for Custom2<'_> {}

mod provider {
    use co3::ffi;

    use super::*;

    #[derive(TypeSpec, ReprC)]
    #[reprC(id(u16))]
    struct Custom1(usize);

    #[derive(TypeSpec, ReprC)]
    #[reprC(id(u16))]
    #[repr(transparent)]
    struct Custom2<'a>(&'a u8);

    handles! {
        unsafe {
            Custom1,
            Custom2<'_>,
        }
    }

    impl Attribute for Custom1 {}
    impl Attribute for Custom2<'_> {}

    impl ByteValue for u8 {
        fn into_byte(self) -> u8 {
            self
        }

        fn add_ref(&self, rhs: &Self) -> u8 {
            *self + *rhs
        }
    }

    impl ByteValue for Custom1 {
        fn into_byte(self) -> u8 {
            self.0 as u8
        }

        fn add_ref(&self, rhs: &Self) -> u8 {
            (self.0 + rhs.0) as u8
        }
    }

    impl ByteValue for Custom2<'_> {
        fn into_byte(self) -> u8 {
            *self.0
        }

        fn add_ref(&self, rhs: &Self) -> u8 {
            self.0 + rhs.0
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #![symbol_prefix = "kita"]

        #[erased(<Custom1>, <Custom2<'_>>)]
        impl<dyn(u16) T: Attribute = EnvAttr> ByteValue for T {
            fn into_byte(self) -> u8;
            fn add_ref(#[soft] &self, #[soft] rhs: &Self) -> u8;
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    #[erased(<Custom1>, <Custom2<'_>>)]
    impl<dyn(u16) T: Attribute = EnvAttr> ByteValue for T {
        fn into_byte(self) -> u8;
        fn add_ref(#[soft] &self, #[soft] rhs: &Self) -> u8;
    }
}

fn main() {
    assert_eq!(Custom1(2).into_byte(), 2);
    assert_eq!(Custom1(2).add_ref(&Custom1(5)), 7);

    assert_eq!(Custom2(&3).into_byte(), 3);
    assert_eq!(Custom2(&3).add_ref(&Custom2(&4)), 7);
}
