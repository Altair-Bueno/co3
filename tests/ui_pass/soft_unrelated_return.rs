use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    fn unrelated<'output>(
        #[soft] value: &mut bool,
        output: &'output bool,
    ) -> &'output bool;
}

fn main() {}
