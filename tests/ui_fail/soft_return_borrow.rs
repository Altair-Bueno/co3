use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    fn direct(#[soft] value: &mut bool) -> &bool;

    fn shortened<'long: 'short, 'short>(
        #[soft] value: &'long mut bool,
        unrelated: &'short (),
    ) -> &'short bool;

    fn from_static<'a>(
        #[soft] value: &'static mut bool,
        unrelated: &'a (),
    ) -> &'a bool;
}

fn main() {}
