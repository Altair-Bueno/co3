use co3::ffi;

trait Selected {
    type Output;
}

impl Selected for u8 {
    type Output = u16;
}

#[unsafe(no_mangle)]
extern "C" fn selected_where_bound_u8() {}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "selected_where_bound_{T}"]
    fn selected_where_bound<T>()
    where
        T: Selected,
        <T as Selected>::Output: Copy,
        use<T> @ <u8>;
}

fn main() {
    selected_where_bound::<u8>();
}
