use co3::{Tag, ffi};

#[derive(Tag)]
#[tag(unsafe(id(u8 = 1)))]
enum First {}

#[derive(Tag)]
#[tag(unsafe(id(u8 = 2)))]
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
