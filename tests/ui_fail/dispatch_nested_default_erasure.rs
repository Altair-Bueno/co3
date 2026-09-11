use co3::{Tag, ffi};

struct Wrapper<T>(T);

#[derive(Tag)]
#[tag(unsafe(id(u8 = 1)))]
struct Value;

ffi! {
    #![unsafe(extern("C"))]

    fn nested<dyn(u8) T = u16>(move value: Wrapper<T>)
    where
        use<T> @ <Value>;
}

fn main() {}
