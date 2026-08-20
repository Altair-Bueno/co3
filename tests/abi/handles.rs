use co3::ffi;

trait Custom {
    fn inc(&mut self, by: u8);
}

unsafe impl co3::handle::Handle for Opaque<bool, u8> {
    const ID: u8 = 1;
}
unsafe impl co3::handle::Handle for Opaque<bool, u32> {
    const ID: u8 = 2;
}
unsafe impl co3::handle::Handle for Opaque<u8, bool> {
    const ID: u8 = 3;
}

ffi! {
    #![unsafe(extern("C"))]

    #[unsafe(id(u8))]
    type Opaque<T, U>;

    impl<T, U> Drop for dyn Opaque<T, U>
    where
        use<T, U> @ <bool, u8>,
    {
        #[symbol_name = "handles_drop"]
        fn drop(&mut self);
    }

    impl Default for OwnedOpaque<bool, u8> {
        #[symbol_name = "handles_default_bool_u8"]
        move fn default() -> Self;
    }

    impl Default for OwnedOpaque<u8, bool> {
        #[symbol_name = "handles_default_u8_bool"]
        move fn default() -> Self;
    }

    impl Clone for OwnedOpaque<bool, u8> {
        #[symbol_name = "handles_clone_bool_u8"]
        move fn clone(&self) -> Self;
    }

    impl<dyn(u8) T> PartialEq for T
    where
        use<T> @ (<Opaque<bool, u8>> | <Opaque<u8, bool>>),
    {
        #[symbol_name = "handles_eq"]
        fn eq(t_id: <dyn T>::ID, &self, other: &Self) -> bool;
    }

    impl<dyn(u8) T, dyn(u8) U> PartialEq<U> for T
    where
        use<T, U> @ <Opaque<bool, u8>, Opaque<u8, bool>>,
    {
        #[symbol_name = "handles_cross_eq"]
        fn eq(t_id: <dyn T>::ID, u_id: <dyn U>::ID, &self, other: &U) -> bool;
    }

    impl<dyn(u8) T> Custom for T
    where
        use<T> @ (<Opaque<bool, u8>> | <Opaque<u8, bool>>),
    {
        #[symbol_name = "handles_inc"]
        fn inc(t_id: <dyn T>::ID, &mut self, by: u8);
    }
}

mod provider {
    use core::marker::PhantomData;

    use co3::ffi;

    use super::*;

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    struct Opaque<T, U> {
        value: u8,
        marker: PhantomData<(T, U)>,
    }

    unsafe impl co3::handle::Handle for Opaque<bool, u8> {
        const ID: u8 = 1;
    }
    unsafe impl co3::handle::Handle for Opaque<bool, u32> {
        const ID: u8 = 2;
    }
    unsafe impl co3::handle::Handle for Opaque<u8, bool> {
        const ID: u8 = 3;
    }

    impl PartialEq<Opaque<u8, bool>> for Opaque<bool, u8> {
        fn eq(&self, other: &Opaque<u8, bool>) -> bool {
            self.value == other.value
        }
    }

    impl<T, U> Custom for Opaque<T, U> {
        fn inc(&mut self, by: u8) {
            self.value += by;
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #[unsafe(id(u8))]
        type Opaque<T, U>;

        impl<T, U> Drop for dyn Opaque<T, U>
        where
            use<T, U> @ (<bool, u8>),
        {
            #[symbol_name = "handles_drop"]
            fn drop(&mut self);
        }

        impl Default for Box<Opaque<bool, u8>> {
            #[symbol_name = "handles_default_bool_u8"]
            move fn default() -> Self;
        }

        impl Default for Box<Opaque<u8, bool>> {
            #[symbol_name = "handles_default_u8_bool"]
            move fn default() -> Self;
        }

        impl Clone for Box<Opaque<bool, u8>> {
            #[symbol_name = "handles_clone_bool_u8"]
            move fn clone(&self) -> Self;
        }

        impl<dyn(u8) T> PartialEq for T
        where
            use<T> @ (<Opaque<bool, u8>> | <Opaque<u8, bool>>),
        {
            #[symbol_name = "handles_eq"]
            fn eq(&self, other: &Self) -> bool;
        }

        impl<dyn(u8) T, dyn(u8) U> PartialEq<U> for T
        where
            use<T, U> @ <Opaque<bool, u8>, Opaque<u8, bool>>,
        {
            #[symbol_name = "handles_cross_eq"]
            fn eq(&self, other: &U) -> bool;
        }

        impl<dyn(u8) T> Custom for T
        where
            use<T> @ (<Opaque<bool, u8>> | <Opaque<u8, bool>>),
        {
            #[symbol_name = "handles_inc"]
            fn inc(&mut self, by: u8);
        }
    }
}

#[test]
fn erased_handle_dispatch() {
    let mut handle: OwnedOpaque<bool, u8> = Default::default();
    let cloned = Clone::clone(&handle);
    assert!(PartialEq::eq(&*handle, &*cloned));

    let other: OwnedOpaque<u8, bool> = Default::default();
    assert!(PartialEq::<Opaque<u8, bool>>::eq(&*handle, &*other));

    Custom::inc(&mut *handle, 2);
    assert!(!PartialEq::<Opaque<u8, bool>>::eq(&*handle, &*other));
    assert!(!PartialEq::eq(&*handle, &*cloned));

    core::mem::forget(other);
}
