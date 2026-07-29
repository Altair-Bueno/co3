use co3::ffi;

mod provider {
    use super::*;

    struct Exported<'a>(&'a u8);

    impl Exported<'_> {
        fn ping(&self) {}
    }

    fn exported_named<'a>(value: &'a u8) -> &'a u8 {
        value
    }

    ffi! {
        #![unsafe(export("C"))]
        fn exported_named<'a>(value: &'a u8) -> &'a u8;

        impl<'a> Exported<'a> {
            fn ping(&self);
        }

        #[explicit_lifetimes]
        impl<'a> Exported<'a> {
            fn pong<'a>(&self);
        }

        #[explicit_lifetimes]
        impl<'a> Exported<'a> {
            #[explicit_lifetimes]
            fn pang<'a>(&self);
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    fn imported_named<'a>(value: &'a u8) -> &'a u8;

    type Imported<'a>;

    impl<'a> Imported<'a> {
        fn ping(&self);
    }

    #[explicit_lifetimes]
    impl<'a> Imported<'a> {
        fn pong<'a>(&self);
    }

    #[explicit_lifetimes]
    impl<'a> Imported<'a> {
        #[explicit_lifetimes]
        fn pang<'a>(&self);
    }
}

fn main() {}
