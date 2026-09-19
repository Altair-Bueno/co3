pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

co3::ffi! {
    #![unsafe(export("C"))]

    #[symbol_name = "rust_super_safe_add"]
    pub fn add(left: u64, right: u64) -> u64;
}
