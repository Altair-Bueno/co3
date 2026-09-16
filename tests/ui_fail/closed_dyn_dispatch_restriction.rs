use co3::{Tag, ffi};

#[derive(Tag)]
#[tag(u8, unsafe(1))]
enum First {}

#[derive(Tag)]
#[tag(u8, unsafe(2))]
enum Unlisted {}

ffi! {
    #![unsafe(extern("C"))]

    pub fn restricted<dyn(u8) T>()
    where
        use<T> @ <First>;
}

fn main() {
    restricted::<Unlisted>();
}
