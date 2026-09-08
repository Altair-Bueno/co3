#![allow(unused_parens)]

use co3::{
    ReprC, ffi,
    slice::{Spread2, TrySpread2},
};
use rust_spec::RustSpec;

trait ExportSpreadLen {
    fn export_trait_spread_len(&self, _: *const u32, len: usize) -> usize;
}

trait ImportSpreadLen {
    fn import_trait_spread_len(&self, values: &[u32]) -> usize;
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Counter(usize);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct OdbcStr<C>([C]);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct PairParts(u8, u8);

impl Spread2<u32, i32> for PairParts {
    fn into_parts(value: Self::CType) -> (u32, i32) {
        (value.0.into(), value.1.into())
    }
}

impl<C: ReprC> TrySpread2<*const C, i16> for &OdbcStr<C>
where
    Self: Spread2<*const C, usize>,
{
    type Error = core::num::TryFromIntError;

    fn try_into_parts(value: Self::CType) -> Result<(*const C, i16), Self::Error> {
        let (data, len) = <Self as Spread2<*const C, usize>>::into_parts(value);
        Ok((data, len.try_into()?))
    }
}

mod c_symbols {
    use super::*;

    fn spread_len_impl(_: *const u32, len: usize) -> usize {
        len
    }

    fn spread_convert_export(data: u32, metadata: i32) -> u32 {
        data + metadata as u32
    }

    fn optional_slice_len_impl(data: *const u32, len: usize) -> usize {
        assert_eq!(data.is_null(), len == 0);
        len
    }

    fn optional_slice_mut_len_impl(data: *mut u32, len: usize) -> usize {
        assert_eq!(data.is_null(), len == 0);
        len
    }

    impl Counter {
        fn inherent_spread_len(&self, _: *const u32, len: usize) -> usize {
            self.0 + len
        }
    }

    impl ExportSpreadLen for Counter {
        fn export_trait_spread_len(&self, _: *const u32, len: usize) -> usize {
            self.0 + len + 1
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #[symbol_name = "spread_len_impl"]
        fn spread_len_impl(_data: *const u32, len: usize) -> usize;

        #[symbol_name = "spread_convert"]
        fn spread_convert_export(data: u32, metadata: i32) -> u32;

        #[symbol_name = "optional_slice_len_impl"]
        fn optional_slice_len_impl(data: *const u32, len: usize) -> usize;

        #[symbol_name = "optional_slice_mut_len_impl"]
        fn optional_slice_mut_len_impl(data: *mut u32, len: usize) -> usize;

        impl Counter {
            #[symbol_name = "inherent_spread_len"]
            fn inherent_spread_len(&self, _data: *const u32, len: usize) -> usize;
        }

        impl ExportSpreadLen for Counter {
            #[symbol_name = "export_trait_spread_len"]
            fn export_trait_spread_len(&self, _data: *const u32, len: usize) -> usize;
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "spread_len_impl"]
    fn spread_len(#[spread(_, _)] values: &[u32]) -> usize;

    #[symbol_name = "spread_convert"]
    fn spread_convert(#[spread(u32, i32)] move value: PairParts) -> u32;

    #[symbol_name = "optional_slice_len_impl"]
    fn optional_slice_len(#[spread(_, _)] values: Option<&[u32]>) -> usize;

    #[symbol_name = "optional_slice_mut_len_impl"]
    fn optional_slice_mut_len(#[try_spread(_, usize)] values: Option<&mut [u32]>) -> usize;

    fn borrowed_box(#[spread(*const u32, usize)] value: Box<[u32]>);
    fn moved_box(#[spread(_, _)] move value: Box<[u32]>);
    fn optional_moved_box(#[spread(_, _)] move value: Option<Box<[u32]>>);
    fn parenthesized_ref(#[spread(_, _)] value: (&[u32]));
    fn parenthesized_tuple(#[spread(_, _)] move value: ((u8, u16)));

    #[symbol_name = "static_spread_{C}"]
    fn static_spread<C>(
        #[try_spread(_, _)] move value: (C, u8),
    )
    where
        use<C> @ (<u8> | <u16>);

    impl Counter {
        #[symbol_name = "inherent_spread_len"]
        fn imported_inherent_spread_len(&self, #[spread(_, _)] values: &[u32]) -> usize;
    }

    impl ImportSpreadLen for Counter {
        #[symbol_name = "export_trait_spread_len"]
        fn import_trait_spread_len(&self, #[try_spread(_, usize)] values: &[u32]) -> usize;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(i16 = 1))]
    type Statement;

    impl Statement {
        #[symbol_name = "static_method_try_spread_dst_{C}"]
        fn prepare<C>(&self, #[try_spread(_, i16)] text: &OdbcStr<C>)
        where
            use<C> @ (<u8> | <u16>);

        #[symbol_name = "static_method_try_spread_dst_{C}_2"]
        fn prepare2<C>(&self, #[try_spread(*const C, i16)] text: &OdbcStr<C>)
        where
            use<C> @ (<u8> | <u16>);

        #[symbol_name = "static_method_multiple_try_spread_dst_{C}"]
        fn prepare_pair<C>(
            &self,
            #[try_spread(_, i16)] first: &OdbcStr<C>,
            #[try_spread(_, i16)] second: &OdbcStr<C>,
        )
        where
            use<C> @ (<u8> | <u16>);
    }
}

mod aliased_core_option {
    use super::*;

    type Option<T> = core::option::Option<T>;

    ffi! {
        #![unsafe(extern("C"))]

        fn aliased_optional_slice(#[spread(_, _)] values: Option<&[u32]>);
    }
}

fn main() {
    assert_eq!(spread_len(&[1, 2, 3]), 3);
    assert_eq!(spread_convert(PairParts(2, 3)), 5);
    assert_eq!(optional_slice_len(Some(&[1, 2, 3])), 3);
    assert_eq!(optional_slice_len(None), 0);

    let mut values = [1, 2, 3, 4];
    assert_eq!(optional_slice_mut_len(Some(&mut values)), 4);
    assert_eq!(optional_slice_mut_len(None), 0);

    let counter = Counter(10);
    assert_eq!(counter.imported_inherent_spread_len(&[1, 2, 3]), 13);
    assert_eq!(counter.import_trait_spread_len(&[1, 2, 3]), 14);
}
