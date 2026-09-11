use co3::{Tag, ffi};

trait Field<H>: co3::tag::Tagged<Kind = u8> {}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u8 = 1))]
    type Resource<V>;

    impl<V> Drop for Resource<V> {
        fn drop(&mut self);
    }

    impl<V, dyn(u8) H> H
    where
        use<H> @ <Resource<V>>,
    {
        fn get<dyn(u8) F: Field<H>>(field: <dyn F>::ID);
    }
}

#[derive(Tag)]
#[tag(unsafe(id(u8 = 2)))]
enum CustomField {}

impl<V> Field<Resource<V>> for CustomField {}

fn main() {}
