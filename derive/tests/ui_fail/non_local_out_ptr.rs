use co3::{ReprC, extern_C};

#[derive(Clone, ReprC)]
pub struct ReprRustStruct(String);

#[derive(Clone, ReprC)]
#[repr(transparent)]
pub enum ReprRustEnum {
    A(String),
}

extern_C! {
    pub fn return_no_repr_struct() -> Vec<ReprRustStruct>;
    pub extern "C" fn return_no_repr_enum() -> Vec<ReprRustEnum>;
}

mod provider {
    use co3::export;

    use super::*;

    #[export("C")]
    pub fn return_no_repr_struct() -> Vec<ReprRustStruct> {
        unimplemented!()
    }

    #[export("C")]
    pub extern "C" fn return_no_repr_enum() -> Vec<ReprRustEnum> {
        unimplemented!()
    }
}

fn main() {}
