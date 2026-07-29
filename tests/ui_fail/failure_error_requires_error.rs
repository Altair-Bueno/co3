use co3::ffi;

fn missing_error_return() -> u8 {
    0
}

ffi! {
    #![unsafe(export("C"))]
    #![failure = "error"]

    fn missing_error_return() -> u8;
}

fn main() {}
