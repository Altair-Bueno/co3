use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    fn unrelated<'soft, 'output>(
        #[soft] value: &'soft mut bool,
        output: &'output bool,
    ) -> &'output bool;
}

fn main() {}
