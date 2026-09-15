use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(covariant('missing))]
    type Unknown<'a>;
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(covariant('a, 'a))]
    type Duplicate<'a>;
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(covariant(T))]
    type TypeParameter<T>;
}

fn main() {}
