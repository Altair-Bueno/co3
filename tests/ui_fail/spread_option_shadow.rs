use co3::ffi;

type Option<T> = Result<T, ()>;

ffi! {
    #![unsafe(extern("C"))]

    fn shadowed_option(#[spread(_, _)] value: Option<&[u8]>);
}

fn main() {}
