use co3::ffi;

type Option<T> = Result<T, ()>;

ffi! {
    #![unsafe(extern("C"))]

    fn shadowed_option(#[unpack_as(_, _)] value: Option<&[u8]>);
}

fn main() {}
