use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    fn tagged_option(#[unpack(_, _)] value: Option<(u8, u8)>);
}

fn main() {}
