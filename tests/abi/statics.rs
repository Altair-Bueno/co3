use co3::ffi;

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "co3_static"]

    pub static VERSION: u32 = 7;
    #[symbol_name = "co3_static_flags"]
    pub static mut FLAGS: u32 = 0;
}

mod imported {
    use co3::ffi;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "co3_static"]

        pub static VERSION: u32;
        #[symbol_name = "co3_static_flags"]
        pub static mut FLAGS: u32;
    }
}

static_assertions::assert_impl_all!(__Co3Static_VERSION: Copy, Clone, Send, Sync);
static_assertions::assert_impl_all!(__Co3Static_FLAGS: Copy, Clone, Send);
static_assertions::assert_not_impl_any!(__Co3Static_FLAGS: Sync);

#[test]
fn immutable_static_has_the_imported_abi() {
    assert_eq!(*VERSION, 7);
    assert_eq!(*imported::VERSION.get().unwrap(), 7);
    assert_eq!(*unsafe { imported::VERSION.get_unchecked() }, 7);
    assert_eq!(imported::VERSION.read(), Some(7));
}

#[test]
fn mutable_static_has_the_imported_abi() {
    unsafe {
        imported::FLAGS.set(3);
        let flags = imported::FLAGS.read().unwrap();
        assert_eq!(flags, 3);
        assert_eq!(FLAGS.read(), Some(3));
        assert_eq!(imported::FLAGS.take(), Some(3));
        assert_eq!(FLAGS.read(), Some(0));
    }
}
