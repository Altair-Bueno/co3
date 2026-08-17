use co3::{ffi, handle::{Handle, HandleFamily}};

struct Version;
struct Attribute;

ffi! {
    #![unsafe(extern("C"))]

    #[id(u8)]
    type Environment<V>;

    impl<V> Drop for dyn Environment<V>
    where
        use<V> @ <Version>,
    {
        fn drop(&mut self);
    }

    impl<V> Environment<V> {
        fn get_attr<dyn(u8) A>(handle: &Self)
        where
            use<A> @ <Attribute>;
    }
}

unsafe impl<V> Handle for Environment<V> {
    const ID: Self::Kind = 1;
}

impl HandleFamily for Attribute {
    type Kind = u8;
}

unsafe impl Handle for Attribute {
    const ID: u8 = 2;
}

fn main() {}
