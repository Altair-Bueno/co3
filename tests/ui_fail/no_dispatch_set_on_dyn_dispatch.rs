use co3::{Handle, ReprC, ffi};
use rust_spec::RustSpec;

#[derive(RustSpec, Handle, ReprC)]
#[handle(unsafe(id(u8 = 1)))]
#[repr(transparent)]
struct First(u8);

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u32))]
    type Kita<T>;

    impl<dyn(u8) T> Drop for Kita<T>
    where
        use<T> @ <First>
    {
        fn drop(&mut self);
    }

    impl<T> dyn Kita<T> {
        pub fn accessible<dyn(u8) U>(value: &U)
        where
            use<U> @ <First>;
    }

    #[symbol_name = "accessible"]
    pub fn accessible<dyn(u8) T = First>(value: &T)
    where
        use<T> @ (<First>);
}

fn assert_dispatch_set<T>()
where
    (): accessible::DispatchSet<T>,
{
}

fn main() {
    assert_dispatch_set::<First>();
}
