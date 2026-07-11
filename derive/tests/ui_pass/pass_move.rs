use co3::{ReprC, ffi};

#[derive(Clone, Debug, PartialEq, Eq, ReprC)]
#[repr(C)]
struct Value(Box<u32>);

mod provider {
    use super::*;

    impl Value {
        fn transform(self, other: &Self) -> Self {
            Self(Box::new(*self.0 + *other.0))
        }
    }

    fn combine(input: &Value, rhs: Value) -> Value {
        Value(Box::new(*input.0 + *rhs.0))
    }

    ffi! {
        #![export("C")]

        impl Value {
            #[symbol_name = "transform"]
            fn transform(move self, move other: &Self) -> Self;
        }

        #[symbol_name = "combine"]
        fn combine(input: &Value, move rhs: Value) -> Value;
    }
}

ffi! {
    #![extern("C")]

    #![expect(unused_doc_comments)]
    //! Documentation

    impl Value {
        /// Documentation
        #[symbol_name = "transform"]
        fn transform2(move self: Self, other: &Self) -> Self;
    }

    /// Documentation
    #[symbol_name = "combine"]
    fn combine(move input: &Value, move rhs: Value) -> Value;
}

fn main() {
    let lhs = Value(2.into());
    let rhs = Value(3.into());

    assert_eq!(lhs.clone().transform2(&rhs), Value(5.into()));
    assert_eq!(combine(&lhs, rhs), Value(5.into()));
}
