use co3::{ReprC, ffi};

trait CustomTrait {
    fn return_no_repr_struct_ref(&self) -> &ReprRustStruct;
    extern "C" fn return_no_repr_enum_ref(&self) -> &ReprRustEnum;

    fn return_no_repr_struct() -> Vec<ReprRustStruct>;
    extern "C" fn return_no_repr_enum() -> Vec<ReprRustEnum>;
}

#[derive(Clone, ReprC)]
pub struct ReprRustStruct(String);

#[derive(Clone, ReprC)]
#[repr(transparent)]
pub enum ReprRustEnum {
    A(String),
}

mod provider {
    use super::*;

    impl CustomTrait for u32 {
        fn return_no_repr_struct_ref(&self) -> &ReprRustStruct {
            unimplemented!()
        }

        extern "C" fn return_no_repr_enum_ref(&self) -> &ReprRustEnum {
            unimplemented!()
        }

        fn return_no_repr_struct() -> Vec<ReprRustStruct> {
            unimplemented!()
        }

        extern "C" fn return_no_repr_enum() -> Vec<ReprRustEnum> {
            unimplemented!()
        }
    }
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    impl CustomTrait for i32 {
        fn return_no_repr_struct_ref(&self) -> &ReprRustStruct;
        extern "C" fn return_no_repr_enum_ref(&self) -> &ReprRustEnum;

        fn return_no_repr_struct() -> Vec<ReprRustStruct>;
        extern "C" fn return_no_repr_enum() -> Vec<ReprRustEnum>;
    }
}

fn main() {}
