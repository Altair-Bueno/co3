use co3::{ReprC, ffi, rust_spec::RustSpec};
type CCallback = extern "C" fn(Value, Value) -> Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, RustSpec, ReprC)]
#[reprC(identity)]
#[repr(C)]
struct Value(u8);

extern "C" fn sum_pair(left: Value, right: Value) -> Value {
    Value(left.0 + right.0)
}

fn apply_callback(callback: CCallback, left: Value, right: Value) -> Value {
    callback(left, right)
}

fn apply_optional_callback(callback: Option<CCallback>, value: Value) -> Value {
    callback.map_or(value, |call| call(value, Value(1)))
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "abi_c_callback"]

    fn apply_callback(callback: CCallback, left: Value, right: Value) -> Value;
    fn apply_optional_callback(callback: Option<CCallback>, value: Value) -> Value;
}

mod imported {
    use co3::ffi;

    use super::*;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_c_callback"]

        pub fn apply_callback(callback: super::CCallback, left: Value, right: Value) -> Value;
        pub fn apply_optional_callback(callback: Option<super::CCallback>, value: Value) -> Value;
    }
}

#[test]
fn c_callback_crosses_export_and_import() {
    assert_eq!(
        imported::apply_callback(sum_pair, Value(19), Value(23)),
        Value(42)
    );
    assert_eq!(
        imported::apply_optional_callback(Some(sum_pair), Value(41)),
        Value(42)
    );
    assert_eq!(
        imported::apply_optional_callback(None, Value(41)),
        Value(41)
    );
}
