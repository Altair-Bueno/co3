use co3::{ExternC, ReprC};
use rust_spec::RustSpec;

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(transparent)]
enum TransparentTupleEnum {
    A(u8),
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(transparent)]
enum TransparentNamedEnum {
    A { value: u8 },
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(transparent)]
enum TransparentMultipleFieldsEnum {
    A((), u8),
}

#[derive(Clone, Copy, RustSpec, ReprC)]
enum ImplicitTransparentTupleEnum {
    A(u8),
}

const _: () = assert!(
    core::mem::size_of::<<TransparentTupleEnum as ExternC>::CType>() == core::mem::size_of::<u8>()
);

const _: () = assert!(
    core::mem::size_of::<<TransparentNamedEnum as ExternC>::CType>() == core::mem::size_of::<u8>()
);

const _: () = assert!(
    core::mem::size_of::<<TransparentMultipleFieldsEnum as ExternC>::CType>()
        == core::mem::size_of::<u8>()
);

const _: () = assert!(
    core::mem::size_of::<<ImplicitTransparentTupleEnum as ExternC>::CType>()
        == core::mem::size_of::<u8>()
);

fn main() {}
