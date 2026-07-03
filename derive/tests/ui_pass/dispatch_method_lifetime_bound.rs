use co3::{ReprC, extern_C, handle::HandleFamily, handles};

trait Attribute {}

trait Dispatch {
    fn len(values: &[u32], attr: &Self) -> usize;
}

#[derive(ReprC)]
#[repr(transparent)]
struct CustomAttribute(u32);

#[derive(ReprC)]
#[reprC(id(u8))]
#[repr(transparent)]
struct CustomAttributeRef<'a>(&'a u32);

impl Attribute for &CustomAttribute {}
impl HandleFamily for &CustomAttribute {
    type Kind = u8;
}

handles! {
    &CustomAttribute,
}

mod provider {
    use co3::export_C;

    use super::*;

    trait Dispatch {
        fn len<T>(values: &[u32], attr: &&T) -> usize;
    }

    impl Dispatch for &CustomAttribute {
        fn len<T>(_values: &[u32], _attr: &&T) -> usize {
            3
        }
    }

    export_C! {
        #[unsafe(lifetimes)]
        #[dispatch(<&CustomAttribute>)]
        impl<'a, dyn(u8) T: 'a + 'a> Dispatch for T
        where
            T: Attribute + 'a + 'a
        {
            #[unsafe(export_name = "len")]
            fn len(values: &[u32], #[soft] attr: &T) -> usize;
        }
    }
}

extern_C! {
    #[unsafe(lifetimes)]
    #[dispatch(<&CustomAttribute>)]
    impl<'a, dyn(u8) T: Attribute + 'a> Dispatch for T {
        #[link_name = "len"]
        fn len(handle_id: <dyn T>::ID, values: &[u32], attr: &T) -> usize;
    }
}

fn main() {
    assert_eq!(<&CustomAttribute>::len(&[1, 2, 3], &&CustomAttribute(0)), 3);
}
