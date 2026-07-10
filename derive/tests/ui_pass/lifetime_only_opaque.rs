use co3::extern_C;

extern_C! {
    #![symbol_prefix = "kita"]

    pub type Opaque<'a>;

    #[unsafe(lifetimes)]
    impl<'a> Default for OwnedOpaque<'a> {
        #[symbol_name = "kita__Default__Box_Opaque__default"]
        fn default() -> Self;
    }

    impl Opaque<'_> {
        fn ping(&self);
    }
}

mod provider {
    use co3::{export, export_C};

    #[export("C", symbol_prefix = "kita")]
    struct Opaque<'a>(&'a u8);

    impl Default for Box<Opaque<'_>> {
        fn default() -> Self {
            Box::new(Opaque(&0))
        }
    }

    impl Opaque<'_> {
        fn ping(&self) {}
    }

    export_C! {
        #![symbol_prefix = "kita"]

        // TODO: This should be allowed with '_ but it's not.
        // This is a special case where reference is materialized
        #[unsafe(lifetimes)]
        impl<'a> Default for Box<Opaque<'a>> {
            #[symbol_name = "kita__Default__Box_Opaque__default"]
            fn default() -> Self;
        }

        impl Opaque<'_> {
            fn ping(&self);
        }
    }
}

fn main() {
    OwnedOpaque::default().ping();
}
