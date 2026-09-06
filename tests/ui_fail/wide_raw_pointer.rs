use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    fn const_wide_pointer(value: *const [u8]);
    fn mut_wide_pointer(value: *mut [u8]);
}

fn main() {}
