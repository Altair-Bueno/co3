use core::mem::MaybeUninit;

use co3::ffi;

type TagKind = u8;

trait ToOwnedHandle: co3::tag::Tagged {
    type Owned;
}

trait Allocate: ToOwnedHandle {
    type Source: ?Sized;
}

trait Version {}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(TagKind = 1))]
    type Parent;

    #[unsafe(id(TagKind = 2))]
    type Child<'parent>;

    #[unsafe(id(TagKind = 3))]
    type GenericParent<V: Version>;

    #[unsafe(id(TagKind = 4))]
    type GenericChild<'parent, V: Version>;

    #[unsafe(id(TagKind = 5))]
    type GenericOther<'parent, 'data, V: Version>;

    #[unsafe(id(TagKind = 6))]
    type GenericRoot<V: Version>;

    impl Drop for dyn Parent {
        fn drop(&mut self);
    }

    impl Drop for dyn Child<'_> {
        fn drop(&mut self);
    }

    impl<V: Version> Drop for dyn GenericParent<V> {
        fn drop(&mut self);
    }

    impl<V: Version> Drop for dyn GenericChild<'_, V> {
        fn drop(&mut self);
    }

    impl<V: Version> Drop for dyn GenericOther<'_, '_, V> {
        fn drop(&mut self);
    }

    impl<V: Version> Drop for dyn GenericRoot<V> {
        fn drop(&mut self);
    }

    fn allocate_generic<'parent, dyn(TagKind) H: Allocate, V: Version>(
        source: Option<&'parent H::Source>,
        output: &mut MaybeUninit<H::Owned>,
    )
    where
        use<H> @ (
            <GenericRoot<V>> |
            <GenericChild<'parent, V>> |
            <GenericOther<'_, '_, V>>
        );
}

impl<'parent> ToOwnedHandle for Child<'parent> {
    type Owned = OwnedChild<'parent>;
}

impl<'parent> Allocate for Child<'parent> {
    type Source = Parent;
}

impl<'parent, V: Version> ToOwnedHandle for GenericChild<'parent, V> {
    type Owned = OwnedGenericChild<'parent, V>;
}

impl<'parent, V: Version> Allocate for GenericChild<'parent, V> {
    type Source = GenericParent<V>;
}

impl<'parent, 'data, V: Version> ToOwnedHandle for GenericOther<'parent, 'data, V> {
    type Owned = OwnedGenericOther<'parent, 'data, V>;
}

impl<'parent, 'data, V: Version> Allocate for GenericOther<'parent, 'data, V> {
    type Source = GenericParent<V>;
}

impl<V: Version> ToOwnedHandle for GenericRoot<V> {
    type Owned = OwnedGenericRoot<V>;
}

impl<V: Version> Allocate for GenericRoot<V> {
    type Source = GenericParent<V>;
}

fn main() {}
