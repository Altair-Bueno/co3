use co3::extern_C;

trait Kita {
    fn kita();
}

mod provider {
    use co3::export_C;

    export_C! {
        #[dispatch(<u32>)]
        #[unsafe(lifetimes)]
        impl<'a, dyn(u8) T> Kita for (u32, T)
        where
            T: 'a,
        {
            #[unsafe(lifetimes)]
            fn kita<'b>()
            where
                T: 'b;
        }

        #[dispatch(<u32>)]
        #[unsafe(lifetimes)]
        impl<'a, 'b, dyn(u8) T> Kita for (i32, T)
        where
            T: 'a + 'b,
        {
            #[unsafe(lifetimes)]
            fn kita();
        }

        #[dispatch(<u32>)]
        #[unsafe(lifetimes)]
        impl<'a, 'b, dyn(u8) T: 'a + 'b> Kita for (u8, T) {
            #[unsafe(lifetimes)]
            fn kita();
        }

        #[dispatch(<u32>)]
        #[unsafe(lifetimes)]
        impl<dyn(u8) T> Kita for (u8, T) {
            #[unsafe(lifetimes)]
            fn kita<'a, 'b>()
            where
                T: 'a + 'b;
        }

        #[dispatch(<u32>)]
        #[unsafe(lifetimes)]
        impl<'a, dyn(u8) T> Kita for (i8, T)
        where
            T: 'a + 'a,
        {
            #[unsafe(lifetimes)]
            fn kita();
        }
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<u32>)]
    #[unsafe(lifetimes)]
    impl<'a, dyn(u8) T> Kita for (u32, T)
    where
        T: 'a,
    {
        #[unsafe(lifetimes)]
        fn kita<'b>(handle_id: <dyn T>::ID)
        where
            T: 'b;
    }

    #[dispatch(<u32>)]
    #[unsafe(lifetimes)]
    impl<'a, 'b, dyn(u8) T> Kita for (i32, T)
    where
        T: 'a + 'b,
    {
        #[unsafe(lifetimes)]
        fn kita(handle_id: <dyn T>::ID);
    }

    #[dispatch(<u32>)]
    #[unsafe(lifetimes)]
    impl<'a, 'b, dyn(u8) T: 'a + 'b> Kita for (u8, T) {
        #[unsafe(lifetimes)]
        fn kita(handle_id: <dyn T>::ID);
    }

    #[dispatch(<u32>)]
    #[unsafe(lifetimes)]
    impl<dyn(u8) T> Kita for (u8, T) {
        #[unsafe(lifetimes)]
        fn kita<'a, 'b>(handle_id: <dyn T>::ID)
        where
            T: 'a + 'b;
    }

    #[dispatch(<u32>)]
    #[unsafe(lifetimes)]
    impl<'a, dyn(u8) T> Kita for (i8, T)
    where
        T: 'a + 'a,
    {
        #[unsafe(lifetimes)]
        fn kita(handle_id: <dyn T>::ID);
    }
}
fn main() {}
