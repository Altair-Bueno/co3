use co3::{Handle, ffi};

struct Version;
#[derive(Handle)]
#[handle(unsafe(id(u8 = 2)))]
struct Attribute;

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u8 = 1))]
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

fn main() {}
