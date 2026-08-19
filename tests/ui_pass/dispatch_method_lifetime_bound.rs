use co3::{
    ReprC, ffi,
    handle::{Handle, HandleFamily},
    rust_spec::RustSpec,
};

trait Attribute {}

trait Dispatch {
    fn len(values: &[u32], attr: &Self) -> usize;
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct CustomAttribute(u32);

#[derive(RustSpec, ReprC)]
#[reprC(unsafe(id(u8)))]
#[repr(transparent)]
struct CustomAttributeRef<'a>(&'a u32);

impl Attribute for &CustomAttribute {}
impl HandleFamily for &CustomAttribute {
    type Kind = u8;
}

unsafe impl Handle for &CustomAttribute {
    const ID: u8 = 1;
}

mod provider {
    use co3::ffi;

    use super::*;

    trait Dispatch {
        fn len<T>(values: &[u32], attr: &&T) -> usize;
    }

    impl Dispatch for &CustomAttribute {
        fn len<T>(_values: &[u32], _attr: &&T) -> usize {
            3
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #[explicit_lifetimes]
        impl<'a, dyn(u8) T: 'a + 'a> Dispatch for T
        where
            T: Attribute + 'a + 'a,
            use<T> @ <&CustomAttribute>,
        {
            #[symbol_name = "len"]
            fn len(values: &[u32], #[soft] attr: &T) -> usize;
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[explicit_lifetimes]
    impl<'a, dyn(u8) T: Attribute + 'a> Dispatch for T
    where
        use<T> @ <&CustomAttribute>,
    {
        #[symbol_name = "len"]
        fn len(handle_id: <dyn T>::ID, values: &[u32], attr: &T) -> usize;
    }
}

fn main() {
    assert_eq!(<&CustomAttribute>::len(&[1, 2, 3], &&CustomAttribute(0)), 3);
}
