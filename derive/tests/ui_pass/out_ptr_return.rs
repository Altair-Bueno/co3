use core::fmt;

use co3::{Decode, Encode, ExternC, FfiReturn, ReprC, export_C, extern_C};

#[derive(Clone, PartialEq)]
struct MyOutPtrReturn(FfiReturn);

impl fmt::Display for MyOutPtrReturn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ExternC for MyOutPtrReturn {
    type CType = i8;
}

unsafe impl co3::stored::EncodeOwned for MyOutPtrReturn {
    type Store = ();

    fn soft_encode<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        co3::encode(self.0)
    }
}

unsafe impl<'d> co3::stored::DecodeOwned<'d> for MyOutPtrReturn {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &'itm mut ()) -> Option<Self> {
        unsafe { co3::decode(source) }.map(Self)
    }
}

impl Encode for MyOutPtrReturn {}
impl Decode<'_> for MyOutPtrReturn {}

impl co3::out_ptr::OutPtrReturn for MyOutPtrReturn {
    fn ok() -> Self {
        Self(FfiReturn::Ok)
    }

    fn unrecoverable_error() -> Self {
        Self(FfiReturn::UnrecoverableError)
    }

    fn trap_representation() -> Self {
        Self(FfiReturn::TrapRepresentation)
    }

    fn unknown_handle() -> Self {
        Self(FfiReturn::UnknownHandle)
    }
}

#[derive(ReprC)]
#[repr(transparent)]
struct Value(u8);

impl Value {
    fn get(&self) -> u8 {
        self.0
    }
}

export_C! {
    impl Value {
        type OutPtrReturn = MyOutPtrReturn;

        fn get(&self) -> u8;
    }
}

extern_C! {
    #![symbol_prefix = "out_ptr_return"]

    impl Value {
        type OutPtrReturn = MyOutPtrReturn;

        fn imported(&self) -> u8;
    }
}

fn main() {}
