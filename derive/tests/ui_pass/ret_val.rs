use co3::{ReprC, export, export_C, extern_C};

#[derive(Clone, ReprC)]
#[repr(transparent)]
struct Value(u8);

impl Value {
    fn new(id: u8) -> Self {
        Self(id)
    }

    fn get(&self) -> u8 {
        self.0
    }
}

#[export("C", symbol_prefix = "ret_val")]
#[ret_val]
#[by_val]
fn exported_attr_value(id: u8) -> Value {
    Value(id)
}

#[export("C", symbol_prefix = "ret_val")]
impl Value {
    #[ret_val]
    fn exported_attr_get(&self) -> u8 {
        self.0
    }
}

export_C! {
    #![export(crate = "ret_val")]

    impl Value {
        #[ret_val]
        fn new(id: u8) -> Self;

        #[ret_val]
        fn get(&self) -> u8;
    }
}

extern_C! {
    #![symbol_prefix = "ret_val"]

    #[ret_val]
    fn imported_value(id: u8) -> Value;

    impl Value {
        #[ret_val]
        fn imported_new(id: u8) -> Self;

        #[ret_val]
        fn imported_get(&self) -> u8;
    }
}

trait ByteValue {
    fn byte(&self) -> u8;
}

trait Attribute {}

#[derive(ReprC)]
#[reprC(id(u8))]
struct Custom(usize);

co3::handles! {
    Custom,
}

impl Attribute for Custom {}

impl ByteValue for Custom {
    fn byte(&self) -> u8 {
        7
    }
}

export_C! {
    #![export(crate = "ret_val")]

    #[dispatch(<Custom>)]
    impl<dyn(u8) T: Attribute = u8> ByteValue for T {
        #[ret_val]
        fn byte(#[soft] &self) -> u8;
    }
}

fn main() {}
