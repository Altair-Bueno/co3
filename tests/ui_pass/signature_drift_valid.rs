use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "signature_drift_valid"]

    #[symbol_name = "extern_fn"]
    fn extern_fn(arg: &Box<u32>);
}

fn main() {}
