use std::ops::Deref;

use co3::{family::TypeFamily, ReprC, ffi, handles};

trait ExternImplTrait {
    fn method(arg: &u32);
}

trait ExportDispatchTrait {
    fn dispatch(&self, arg: &u32);
}

trait ExternDispatchTrait {
    fn dispatch(&self, arg: &u32);
}

struct ExportImpl;
struct ExternImpl;

#[derive(TypeFamily, ReprC)]
#[reprC(id(u8))]
#[repr(transparent)]
struct DriftHandle(u32);

fn export_fn(_: &u32) {}

impl ExportImpl {
    fn method(_: &u32) {}
}

impl Deref for DriftHandle {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ExportDispatchTrait for DriftHandle {
    fn dispatch(&self, _: &u32) {}
}

handles! {
    unsafe {
        DriftHandle,
    }
}

ffi! {
    #![unsafe(export("C"))]

    fn export_fn(arg: &Box<u32>);
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "signature_drift"]

    #[symbol_name = "extern_fn"]
    fn extern_fn(arg: &Box<u32>);
}

ffi! {
    #![unsafe(export("C"))]

    impl ExportImpl {
        fn method(arg: &Box<u32>);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "signature_drift"]

    impl ExternImplTrait for ExternImpl {
        fn method(arg: &Box<u32>);
    }
}

ffi! {
    #![unsafe(export("C"))]

    #[erased(<DriftHandle>)]
    impl<dyn(u8) T = DriftHandle> ExportDispatchTrait for T {
        fn dispatch(&self, arg: &T);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "signature_drift"]

    #[erased(<DriftHandle>)]
    impl<dyn(u8) T = DriftHandle> ExternDispatchTrait for T {
        fn dispatch(self_id: <dyn Self>::ID, &self, arg: &T);
    }
}

fn main() {}
