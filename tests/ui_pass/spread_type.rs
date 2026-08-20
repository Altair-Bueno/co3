use co3::{ReprC, ffi};
use rust_spec::RustSpec;

trait ExportSpreadLen {
    fn export_trait_spread_len(&self, _: *const u32, len: u16) -> usize;
}

trait ImportSpreadLen {
    fn import_trait_spread_len(&self, values: &[u32]) -> usize;
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Counter(usize);

mod c_symbols {
    use super::*;

    fn spread_len_impl(_: *const u32, len: usize) -> usize {
        len
    }

    fn spread_convert_export(data: u32, metadata: i32) -> u32 {
        data + metadata as u32
    }

    impl Counter {
        fn inherent_spread_len(&self, _: *const u32, len: usize) -> usize {
            self.0 + len
        }
    }

    impl ExportSpreadLen for Counter {
        fn export_trait_spread_len(&self, _: *const u32, len: u16) -> usize {
            self.0 + len as usize + 1
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #[symbol_name = "spread_len_impl"]
        fn spread_len_impl(_data: *const u32, len: usize) -> usize;

        #[symbol_name = "spread_convert"]
        fn spread_convert_export(data: u32, metadata: i32) -> u32;


        impl Counter {
            #[symbol_name = "inherent_spread_len"]
            fn inherent_spread_len(&self, _data: *const u32, len: usize) -> usize;
        }

        impl ExportSpreadLen for Counter {
            #[symbol_name = "export_trait_spread_len"]
            fn export_trait_spread_len(&self, _data: *const u32, len: u16) -> usize;
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "spread_len_impl"]
    fn spread_len(#[spread(_, _)] values: &[u32]) -> usize;

    #[symbol_name = "spread_convert"]
    fn spread_convert(#[spread(u32, i32)] value: co3::tuple::ReprCTuple2<u8, u8>) -> u32;


    impl Counter {
        #[symbol_name = "inherent_spread_len"]
        fn imported_inherent_spread_len(&self, #[spread(_, _)] values: &[u32]) -> usize;
    }

    impl ImportSpreadLen for Counter {
        #[symbol_name = "export_trait_spread_len"]
        fn import_trait_spread_len(&self, #[try_spread(_, u16)] values: &[u32]) -> usize;
    }
}

fn main() {
    assert_eq!(spread_len(&[1, 2, 3]), 3);
    assert_eq!(spread_convert(co3::tuple::ReprCTuple2(2, 3)), 5);

    let counter = Counter(10);
    assert_eq!(counter.imported_inherent_spread_len(&[1, 2, 3]), 13);
    assert_eq!(counter.import_trait_spread_len(&[1, 2, 3]), 14);
}
