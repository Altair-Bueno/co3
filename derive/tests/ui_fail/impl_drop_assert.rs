use co3::{rust_spec::TypeSpec, ReprC};

#[derive(TypeSpec, ReprC)]
#[repr(u8)]
enum PrimitiveEnumWithoutDrop {
    A,
    B,
}

#[derive(TypeSpec, ReprC)]
#[repr(u8)]
enum PrimitiveEnumWithDrop {
    A,
    B,
}

impl Drop for PrimitiveEnumWithDrop {
    fn drop(&mut self) {}
}

#[derive(Clone, TypeSpec, ReprC)]
#[repr(transparent)]
enum TransparentEnumWithoutDrop {
    A(String),
}

#[derive(Clone, TypeSpec, ReprC)]
#[repr(transparent)]
enum TransparentEnumWithDrop {
    A(String),
}

impl Drop for TransparentEnumWithDrop {
    fn drop(&mut self) {}
}

#[derive(TypeSpec, ReprC)]
#[repr(C, u8)]
enum ReprCDataEnumWithoutDrop {
    A(u8),
}

#[derive(TypeSpec, ReprC)]
#[repr(C, u8)]
enum ReprCDataEnumWithDrop {
    A(u8),
}

impl Drop for ReprCDataEnumWithDrop {
    fn drop(&mut self) {}
}

#[derive(TypeSpec, ReprC)]
#[repr(u8)]
enum DataEnumWithoutDrop {
    A(u8),
}

#[derive(TypeSpec, ReprC)]
#[repr(u8)]
enum DataEnumWithDrop {
    A(u8),
}

impl Drop for DataEnumWithDrop {
    fn drop(&mut self) {}
}

#[derive(TypeSpec, ReprC)]
enum ReprRustDataEnumWithoutDrop {
    A(u8),
}

#[derive(TypeSpec, ReprC)]
enum ReprRustDataEnumWithDrop {
    A(u8),
}

impl Drop for ReprRustDataEnumWithDrop {
    fn drop(&mut self) {}
}

#[derive(TypeSpec, ReprC)]
#[repr(C)]
struct ReprCStructWithoutDrop(u8);

#[derive(TypeSpec, ReprC)]
#[repr(C)]
struct ReprCStructWithDrop(u8);

impl Drop for ReprCStructWithDrop {
    fn drop(&mut self) {}
}

#[derive(Clone, TypeSpec, ReprC)]
#[repr(transparent)]
struct TransparentStructWithoutDrop(u8);

#[derive(Clone, TypeSpec, ReprC)]
#[repr(transparent)]
struct TransparentStructWithDrop(u8);

impl Drop for TransparentStructWithDrop {
    fn drop(&mut self) {}
}

#[derive(TypeSpec, ReprC)]
struct ReprRustStructWithoutDrop(u8);

#[derive(TypeSpec, ReprC)]
struct ReprRustStructWithDrop(u8);

impl Drop for ReprRustStructWithDrop {
    fn drop(&mut self) {}
}

fn main() {}
