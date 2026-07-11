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
        #![export("C")]
        fn exported_named<'a>(value: &'a u8) -> &'a u8;

        impl<'a> Exported<'a> {
            fn ping(&self);
        }

        #[unsafe(lifetimes)]
        impl<'a> Exported<'a> {
            fn pong<'a>(&self);
        }

        #[unsafe(lifetimes)]
        impl<'a> Exported<'a> {
            #[unsafe(lifetimes)]
            fn pang<'a>(&self);
        }
    }
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    fn imported_named<'a>(value: &'a u8) -> &'a u8;

    type Imported<'a>;

    impl<'a> Imported<'a> {
        fn ping(&self);
    }

    #[unsafe(lifetimes)]
    impl<'a> Imported<'a> {
        fn pong<'a>(&self);
    }

    #[unsafe(lifetimes)]
    impl<'a> Imported<'a> {
        #[unsafe(lifetimes)]
        fn pang<'a>(&self);
    }
}

fn main() {}
