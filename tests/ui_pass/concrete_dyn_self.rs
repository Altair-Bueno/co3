use co3::ffi;

trait Value {
    fn value(&self) -> u8;
}

mod export {
    use super::*;

    struct Concrete(u8);

    impl Value for Concrete {
        fn value(&self) -> u8 {
            self.0
        }
    }

    impl Concrete {
        fn own_value(&mut self) -> u8 {
            self.0
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #[unsafe(id(u8 = 1))]
        type Concrete;

        impl Value for dyn Concrete {
            #[symbol_name = "concrete_dyn_self_value"]
            fn value(&self) -> u8;
        }

        impl dyn Concrete {
            #[symbol_name = "concrete_dyn_self_own_value"]
            fn own_value(&mut self) -> u8;
        }
    }
}

mod import {
    use super::*;

    ffi! {
        #![unsafe(extern("C"))]

        #[unsafe(id(u8 = 1))]
        type Concrete;

        impl Value for dyn Concrete {
            #[symbol_name = "concrete_dyn_self_value"]
            fn value(tag_id: <dyn Self>::ID, &self) -> u8;
        }

        impl dyn Concrete {
            #[symbol_name = "concrete_dyn_self_own_value"]
            fn own_value(tag_id: <dyn Concrete>::ID, &mut self) -> u8;
        }
    }
}

fn main() {}
