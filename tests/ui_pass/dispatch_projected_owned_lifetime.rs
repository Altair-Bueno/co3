use core::mem::MaybeUninit;

use co3::ffi;

type HandleKind = u8;

trait ToOwnedHandle: co3::handle::Handle {
    type Owned;
}

trait Allocate: ToOwnedHandle {
    type Source: ?Sized;
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(HandleKind = 1))]
    type Parent;

    #[unsafe(id(HandleKind = 2))]
    type Child<'parent>;

    impl Drop for dyn Parent {
        fn drop(&mut self);
    }

    impl Drop for dyn Child<'_> {
        fn drop(&mut self);
    }

    fn allocate<'parent, dyn(HandleKind) H: Allocate>(
        source: &'parent H::Source,
        output: &mut MaybeUninit<H::Owned>,
    )
    where
        use<H> @ <Child<'parent>>;
}

impl<'parent> ToOwnedHandle for Child<'parent> {
    type Owned = OwnedChild<'parent>;
}

impl<'parent> Allocate for Child<'parent> {
    type Source = Parent;
}

fn allocate_child<'parent>(parent: &'parent Parent) {
    let mut child = MaybeUninit::uninit();
    allocate::<Child<'parent>>(parent, &mut child);
}

fn main() {}
