use co3::{ReprC, export_, export_C, extern_, extern_C, handles};

#[derive(Clone, ReprC)]
#[repr(transparent)]
struct Value(u8);

fn value_plain(value: Value) -> u8 {
    value.0
}

fn value_soft(value: &(u8,)) -> u8 {
    value.0
}

export_C! {
    #![cfg_attr(any(), symbol_prefix = "unused")]
    #![cfg_attr(all(), symbol_prefix = "cfg_attr")]

    #[cfg_attr(any(), symbol_name = "unused")]
    #[cfg_attr(all(), symbol_name = "cfg_attr__value_plain")]
    fn value_plain(value: Value) -> u8;
}

extern_C! {
    #![cfg_attr(any(), symbol_prefix = "unused")]
    #![cfg_attr(all(), symbol_prefix = "cfg_attr")]

    #[cfg_attr(any(), symbol_name = "unused")]
    #[cfg_attr(all(), symbol_name = "cfg_attr__value_plain")]
    fn imported_value_plain(value: Value) -> u8;
}

export_! {
    #![cfg_attr(all(), abi = "C")]

    #[symbol_name = "cfg_attr__value_soft"]
    fn value_soft(#[cfg_attr(all(), soft)] value: &(u8,)) -> u8;
}

extern_! {
    #![cfg_attr(all(), abi = "C")]

    #[symbol_name = "cfg_attr__value_soft"]
    fn imported_value_soft(#[cfg_attr(all(), soft)] value: &(u8,)) -> u8;
}

struct ExportOpaque;

export_C! {
    #![symbol_prefix = "cfg_attr"]

    #[cfg_attr(all(), id(u8))]
    type ExportOpaque;
}

mod imported {
    use co3::extern_C;

    extern_C! {
        #![symbol_prefix = "cfg_attr"]

        #[cfg_attr(all(), id(u8))]
        type Opaque;
    }
}

trait Attr {}

trait ByteValue {
    fn into_byte(self) -> u8;
}

#[derive(Clone, ReprC)]
#[repr(transparent)]
struct EnvAttr(usize);

#[derive(Clone, ReprC)]
#[reprC(id(u16))]
struct Custom(usize);

handles! {
    Custom,
}

impl Attr for Custom {}

mod provider {
    use co3::{ReprC, export_C, handles};

    use super::{Attr, ByteValue, EnvAttr};

    #[derive(Clone, ReprC)]
    #[reprC(id(u16))]
    pub(super) struct Custom(usize);

    handles! {
        Custom,
    }

    impl Attr for Custom {}

    impl ByteValue for Custom {
        fn into_byte(self) -> u8 {
            self.0 as u8
        }
    }

    export_C! {
        #![symbol_prefix = "cfg_attr_dispatch"]

        #[cfg_attr(all(), dispatch(<Custom>))]
        impl<#[cfg_attr(all(), erased(u16))] T: Attr = EnvAttr> ByteValue for T {
            fn into_byte(self) -> u8;
        }
    }
}

extern_C! {
    #![symbol_prefix = "cfg_attr_dispatch"]

    #[cfg_attr(all(), dispatch(<Custom>))]
    impl<#[cfg_attr(all(), erased(u16))] T: Attr = EnvAttr> ByteValue for T {
        fn into_byte(handle_id: <dyn T>::ID, self) -> u8;
    }
}

fn main() {
    assert_eq!(imported_value_plain(Value(3)), 3);
    assert_eq!(imported_value_soft(&(4,)), 4);
    assert_eq!(Custom(5).into_byte(), 5);
}
