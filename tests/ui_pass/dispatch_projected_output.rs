use co3::{ffi, handles};

trait Projected {
    type Output;

    fn projected(&self) -> Vec<Self::Output>;
}

handles! {
    unsafe {
        First,
        Second,
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u8))]
    type First;

    #[unsafe(id(u8))]
    type Second;

    impl ToOwned for First {
        type Owned = OwnedFirst;

        move fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }

    impl ToOwned for Second {
        type Owned = OwnedSecond;

        move fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }

    impl<dyn(u8) T: ToOwned> Projected for T
    where
        use<T> @ (<First> | <Second>),
    {
        type Output = <T as ToOwned>::Owned;

        move fn projected(&self) -> Vec<<Self as Projected>::Output>;
    }
}

fn main() {}
