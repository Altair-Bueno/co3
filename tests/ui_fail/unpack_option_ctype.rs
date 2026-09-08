use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    fn tagged_option(#[unpack_as(_, _)] value: Option<(u8, u8)>);
}

fn main() {}
