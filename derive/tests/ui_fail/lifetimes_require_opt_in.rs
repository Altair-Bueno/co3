use co3::{export, export_C, extern_C};

mod provider {
    use super::*;

    struct Exported<'a>(&'a u8);

    impl Exported<'_> {
        fn ping(&self) {}
    }

    fn exported_named<'a>(value: &'a u8) -> &'a u8 {
        value
    }

    #[export("C")]
    #[unsafe(lifetimes)]
    fn export_attr_named<'a>(value: &'a u8) -> &'a u8 {
        value
    }

    export_C! {
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

extern_C! {
    #![link(crate = "kita")]

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
