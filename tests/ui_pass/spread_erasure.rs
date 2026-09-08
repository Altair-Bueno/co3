use core::marker::PhantomData;

use co3::{Handle, ReprC, ffi, slice::Spread2};
use rust_spec::RustSpec;

trait Prop {
    type DefinedBy;
}

enum OdbcDefined {}

#[derive(Handle)]
#[handle(unsafe(id(u8 = 1)))]
enum Attribute {}

impl Prop for Attribute {
    type DefinedBy = OdbcDefined;
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct AttrLength<D>(u16, PhantomData<fn() -> D>);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct AttrPointer<D>(u32, PhantomData<fn() -> D>);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Parts(u32, u16);

impl<D> Spread2<u32, CAttrLength<D>> for Parts {
    fn into_parts(value: Self::CType) -> (u32, CAttrLength<D>) {
        (value.0, CAttrLength(value.1, PhantomData))
    }
}

impl<D> Spread2<CAttrPointer<D>, CAttrLength<D>> for Parts {
    fn into_parts(value: Self::CType) -> (CAttrPointer<D>, CAttrLength<D>) {
        (
            CAttrPointer(value.0, PhantomData),
            CAttrLength(value.1, PhantomData),
        )
    }
}

mod symbols {
    #[unsafe(no_mangle)]
    extern "C" fn projected(_: u8, _: u32, _: u16) {}

    #[unsafe(no_mangle)]
    extern "C" fn projected_try(_: u8, _: u32, _: u16) {}

    #[unsafe(no_mangle)]
    extern "C" fn both_parts(_: u32, _: u16) {}
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "projected"]
    fn projected<dyn(u8) A: Prop>(
        move attribute: <dyn A>::ID,
        #[spread(u32, AttrLength<<A as Prop>::DefinedBy> => u16)]
        move value: Parts,
    )
    where
        use<A> @ <Attribute>;

    #[symbol_name = "projected_try"]
    fn projected_try<dyn(u8) A: Prop>(
        move attribute: <dyn A>::ID,
        #[try_spread(u32, AttrLength<<A as Prop>::DefinedBy> => u16)]
        move value: Parts,
    )
    where
        use<A> @ <Attribute>;

    #[symbol_name = "both_parts"]
    fn both_parts(
        #[spread(AttrPointer<OdbcDefined> => u32, AttrLength<OdbcDefined> => u16)]
        move value: Parts,
    );
}

fn main() {
    projected(Parts(7, 11));
    projected_try(Parts(7, 11));
    both_parts(Parts(7, 11));
}
