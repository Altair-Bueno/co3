use co3::{rust_spec::TypeSpec, CFnArg, CFnReturn, ExternC, ReprC};

#[derive(Clone, Copy, TypeSpec, ReprC)]
#[repr(C)]
struct ParamReprCZst<T: ?Sized> {
    a: (),
    b: T,
}

#[derive(Clone, Copy, TypeSpec, ReprC)]
#[repr(transparent)]
struct ParamTransparentZst<T: ?Sized> {
    b: T,
}

#[derive(Clone, Copy, TypeSpec, ReprC)]
#[repr(transparent)]
enum ParamTransparentEnum<T> {
    A(T),
}

#[derive(Clone, Copy, TypeSpec, ReprC)]
enum ParamFieldlessEnum {
    A,
}

// FIXME: This should fail
#[derive(Clone, Copy, TypeSpec, ReprC)]
struct ZeroLenArrayZst {
    field: [u8; 0],
}

#[derive(Clone, Copy, TypeSpec, ReprC)]
struct ParamNoReprZst<T: ?Sized> {
    a: (),
    b: T,
}

#[derive(Clone, Copy, TypeSpec, ReprC)]
#[repr(C)]
struct ReprCZst {
    a: (),
    b: (),
}

#[derive(Clone, Copy, TypeSpec, ReprC)]
#[repr(transparent)]
struct TransparentZst {
    b: (),
}

#[derive(Clone, Copy, TypeSpec, ReprC)]
struct NoReprZst {
    a: (),
    b: (),
}

#[derive(Debug, Clone, Copy, TypeSpec, ReprC)]
#[repr(transparent)]
pub enum FieldlessTransparentEnum {
    A,
}

#[derive(Debug, Clone, Copy, TypeSpec, ReprC)]
#[repr(transparent)]
pub struct UnitTransparentStruct;

fn require_arg<T: CFnArg>() {}
fn require_return<T: CFnReturn>() {}

fn main() {
    require_arg::<<ReprCZst as ExternC>::CType>();
    require_return::<<ReprCZst as ExternC>::CType>();

    require_arg::<<TransparentZst as ExternC>::CType>();
    require_return::<<TransparentZst as ExternC>::CType>();

    require_arg::<<NoReprZst as ExternC>::CType>();
    require_return::<<NoReprZst as ExternC>::CType>();

    require_arg::<<ParamReprCZst<()> as ExternC>::CType>();
    require_return::<<ParamReprCZst<()> as ExternC>::CType>();

    require_arg::<<ParamTransparentZst<()> as ExternC>::CType>();
    require_return::<<ParamTransparentZst<()> as ExternC>::CType>();

    require_arg::<<ParamNoReprZst<()> as ExternC>::CType>();
    require_return::<<ParamNoReprZst<()> as ExternC>::CType>();

    require_arg::<<ParamTransparentEnum<()> as ExternC>::CType>();
    require_return::<<ParamTransparentEnum<()> as ExternC>::CType>();

    require_arg::<<ParamTransparentEnum<u32> as ExternC>::CType>();
    require_return::<<ParamTransparentEnum<u32> as ExternC>::CType>();

    require_arg::<<ParamFieldlessEnum as ExternC>::CType>();
    require_return::<<ParamFieldlessEnum as ExternC>::CType>();
}
