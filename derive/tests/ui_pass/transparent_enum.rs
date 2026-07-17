use co3::{family::TypeFamily, ExternC, ReprC};

#[derive(Clone, Copy, TypeFamily, ReprC)]
#[repr(transparent)]
enum TransparentTupleEnum {
    A(u8),
}

#[derive(Clone, Copy, TypeFamily, ReprC)]
#[repr(transparent)]
enum TransparentNamedEnum {
    A { value: u8 },
}

#[derive(Clone, Copy, TypeFamily, ReprC)]
enum ImplicitTransparentTupleEnum {
    A(u8),
}

const _: () = assert!(
    core::mem::size_of::<<TransparentTupleEnum as ExternC>::CType>()
        == core::mem::size_of::<u8>()
);

const _: () = assert!(
    core::mem::size_of::<<TransparentNamedEnum as ExternC>::CType>()
        == core::mem::size_of::<u8>()
);

const _: () = assert!(
    core::mem::size_of::<<ImplicitTransparentTupleEnum as ExternC>::CType>()
        == core::mem::size_of::<u8>()
);

fn main() {}
