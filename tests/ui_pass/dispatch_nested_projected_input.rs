use core::mem::MaybeUninit;

use co3::ffi;

type HandleKind = u8;

trait Family: co3::handle::Handle {
    type Source;
    type Owned;
}

trait Version {}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(HandleKind = 1))]
    type Parent<V: Version>;

    #[unsafe(id(HandleKind = 2))]
    type Child<V: Version>;

    impl<V: Version> Drop for dyn Parent<V> {
        fn drop(&mut self);
    }

    impl<V: Version> Drop for dyn Child<V> {
        fn drop(&mut self);
    }

    fn nested_projected_input<'src, dyn(HandleKind) H: Family, V: Version>(
        move source: Option<&'src H::Source>,
        output_handle: &mut MaybeUninit<H::Owned>,
    )
    where
        use<H> @ (<Parent<V>> | <Child<V>>);
}

impl<V: Version> Family for Parent<V> {
    type Source = u32;
    type Owned = OwnedParent<V>;
}

impl<V: Version> Family for Child<V> {
    type Source = i32;
    type Owned = OwnedChild<V>;
}

fn main() {}
