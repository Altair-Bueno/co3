use co3::{ReprC, export_C, extern_C};

trait ExportSpreadLen {
    fn export_trait_spread_len(&self, values: &[u32]) -> usize;
}

trait ImportSpreadLen {
    fn import_trait_spread_len(&self, values: &[u32]) -> usize;
}

#[derive(ReprC)]
#[repr(transparent)]
struct Counter(usize);

fn spread_len_impl(values: &[u32]) -> usize {
    values.len()
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

export_C! {
    #[unsafe(export_name = "spread_len")]
    fn spread_len_impl(values: ..&[u32]) -> usize;

    impl Counter {
        #[unsafe(export_name = "counter_inherent_spread_len")]
        fn inherent_spread_len(&self, values: ..&[u32]) -> usize;
    }

    impl ExportSpreadLen for Counter {
        #[unsafe(export_name = "counter_trait_spread_len")]
        fn export_trait_spread_len(&self, values: ..&[u32]) -> usize;
    }
}

extern_C! {
    #[link_name = "spread_len"]
    fn spread_len(values: ..&[u32]) -> usize;

    impl Counter {
        #[link_name = "counter_inherent_spread_len"]
        fn imported_inherent_spread_len(&self, values: ..&[u32]) -> usize;
    }

    impl ImportSpreadLen for Counter {
        #[link_name = "counter_trait_spread_len"]
        fn import_trait_spread_len(&self, values: ..&[u32]) -> usize;
    }
}

fn main() {
    assert_eq!(spread_len(&[1, 2, 3]), 3);

    let counter = Counter(10);
    assert_eq!(counter.imported_inherent_spread_len(&[1, 2, 3]), 13);
    assert_eq!(counter.import_trait_spread_len(&[1, 2, 3]), 14);
}
