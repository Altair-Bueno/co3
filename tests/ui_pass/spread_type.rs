use co3::{rust_spec::RustSpec, ReprC, ffi};

trait ExportSpreadLen {
    fn export_trait_spread_len(&self, values: &[u32]) -> usize;
}

trait ImportSpreadLen {
    fn import_trait_spread_len(&self, values: &[u32]) -> usize;
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Counter(usize);

fn spread_len_impl(values: &[u32]) -> usize {
    values.len()
}

mod c_symbols {
    #[unsafe(no_mangle)]
    extern "C" fn spread_len(_: *const u32, len: usize) -> usize {
        len
    }

    #[unsafe(no_mangle)]
    extern "C" fn counter_inherent_spread_len(
        counter: *const super::Counter,
        _: *const u32,
        len: usize,
    ) -> usize {
        unsafe { (*counter).0 + len }
    }

    #[unsafe(no_mangle)]
    extern "C" fn counter_trait_spread_len(
        counter: *const super::Counter,
        _: *const u32,
        len: usize,
    ) -> usize {
        unsafe { (*counter).0 + len + 1 }
    }
}

impl Counter {
    fn inherent_spread_len(&self, values: &[u32]) -> usize {
        self.0 + values.len()
    }
}

impl ExportSpreadLen for Counter {
    fn export_trait_spread_len(&self, values: &[u32]) -> usize {
        self.0 + values.len() + 1
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "spread_len"]
    fn spread_len(#[spread2] values: &[u32]) -> usize;

    impl Counter {
        #[symbol_name = "counter_inherent_spread_len"]
        fn imported_inherent_spread_len(&self, #[spread2] values: &[u32]) -> usize;
    }

    impl ImportSpreadLen for Counter {
        #[symbol_name = "counter_trait_spread_len"]
        fn import_trait_spread_len(&self, #[spread2] values: &[u32]) -> usize;
    }
}

fn main() {
    assert_eq!(spread_len(&[1, 2, 3]), 3);

    let counter = Counter(10);
    assert_eq!(counter.imported_inherent_spread_len(&[1, 2, 3]), 13);
    assert_eq!(counter.import_trait_spread_len(&[1, 2, 3]), 14);
}
