use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u8, 1)]
    type MissingUnsafeValue;
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(tag(u8))]
    type LegacyUnsafeAttribute;
}

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u8)]
    #[tag(u8)]
    type DuplicateTag;
}

fn main() {}
