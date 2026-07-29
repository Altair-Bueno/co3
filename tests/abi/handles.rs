use co3::ffi;

trait Custom {
    fn inc(self, by: Vec<u32>) -> Self;
}

co3::handles! { unsafe {
    Opaque::<bool, u8> = 1,
    Opaque::<bool, u32>,
    Opaque<u8, bool>,
} }

ffi! {
    #![unsafe(extern("C"))]

    #[id(u8)]
    type Opaque<T, U>;

    #[erased]
    impl<T, U> Drop for dyn Opaque<T, U> {
        #[symbol_name = "drop"]
        fn drop(self_id: <dyn Self>::ID, &mut self);
    }

    #[erased(
        <bool, u8>,
        <u8, bool>
    )]
    impl<T, U> dyn Opaque<T, U> {
        #[symbol_name = "handle_as_ref"]
        fn try_as_ref(id: <dyn Self>::ID, &self) -> Result<&Self, u8>;
    }

    #[erased(
        <Opaque<bool, u8>>,
        <Opaque<u8, bool>>,
    )]
    impl<dyn(u8) T> Clone for T {
        fn clone(self_id: <dyn Self>::ID, &self) -> Self;
    }

    #[erased(
        <Opaque<u8, bool>>,
        <Opaque<bool, u8>>,
    )]
    impl<dyn(u8) T> Default for T {
        #[symbol_name = "default"]
        fn default(self_id: <dyn Self>::ID) -> Self;
    }

    #[erased(
        <bool, u8>,
        <u8, bool>,
    )]
    impl<T, U> PartialEq for dyn Opaque<T, U> {
        fn eq(self_id: <dyn Self>::ID, &self, other: &Self) -> bool;
    }

    #[erased(
        <Opaque<bool, u8>, Opaque<u8, bool>>,
    )]
    impl<dyn(u8) T, dyn(u8) U> PartialEq<U> for T {
        #[symbol_name = "abi_Eq_eq_2"]
        fn eq(self_id: <dyn Self>::ID, other_id: <dyn U>::ID, &self, other: &U) -> bool;
    }

    #[erased(<bool>)]
    impl<T> Custom for dyn Opaque<T, u8> {
        #[symbol_name = "custom_inc_as_ref"]
        fn inc(self_id: <dyn Self>::ID, self, by: Vec<u32>) -> Self;
    }

    // TODO:
    //#[erased(<u8>)]
    //impl<T> Custom for dyn Opaque<T, bool> {
    //    #[symbol_name = "custom_inc_move"]
    //    fn inc(self_id: <dyn Self>::ID, self, move by: Vec<u32>) -> Self;
    //}
}

mod provider {
    use core::marker::PhantomData;

    use co3::{ffi, handles};

    use super::Custom;

    handles! { unsafe {
        Opaque::<bool, u8> = 1,
        Opaque<bool, u32>,
        Opaque<u8, bool>,
    } }

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    pub struct Opaque<T, U> {
        id: u8,
        _marker: PhantomData<(T, U)>,
    }

    impl PartialEq<Opaque<u8, bool>> for Opaque<bool, u8> {
        fn eq(&self, other: &Opaque<u8, bool>) -> bool {
            self.id == other.id
        }
    }

    impl<T, U> Custom for Opaque<T, U> {
        fn inc(mut self, by: Vec<u32>) -> Self {
            by.into_iter().for_each(|by| self.id += by as u8);
            self
        }
    }

    impl<T, U> Opaque<T, U> {
        fn try_as_ref(&self) -> Result<&Self, u8> {
            Ok(self)
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #[id(u8)]
        pub type Opaque<T, U>;

        #[erased(
            <bool, u8>,
            <u8, bool>,
        )]
        impl<T, U> Drop for dyn Opaque<T, U> {
            #[symbol_name = "drop"]
            fn drop(&mut self);
        }

        #[erased(
            <bool, u8>,
            <u8, bool>
        )]
        impl<T, U> dyn Opaque<T, U> {
            #[symbol_name = "handle_as_ref"]
            fn try_as_ref(&self) -> Result<&Self, u8>;
        }

        #[erased(
            <Opaque<bool, u8>>,
            <Opaque<u8, bool>>,
        )]
        impl<dyn(u8) T> Clone for T {
            fn clone(&self) -> Self;
        }

        #[erased(
            <bool, u8>,
            <u8, bool>,
        )]
        impl<T, U> Default for dyn Opaque<T, U> {
            #[symbol_name = "default"]
            fn default() -> Self;
        }

        #[erased(
            <bool, u8>,
            <u8, bool>,
        )]
        impl<T, U> PartialEq for dyn Opaque<T, U> {
            fn eq(&self, other: &Self) -> bool;
        }

        #[erased(
            <Opaque<bool, u8>, Opaque<u8, bool>>,
        )]
        impl<dyn(u8) T, dyn(u8) TU> PartialEq<TU> for T {
            #[symbol_name = "abi_Eq_eq_2"]
            fn eq(&self, other: &TU) -> bool;
        }

        #[erased(<bool>)]
        impl<T> Custom for dyn Opaque<T, u8> {
            #[symbol_name = "custom_inc_as_ref"]
            fn inc(self, by: Vec<u32>) -> Self;
        }

        // TODO:
        //#[erased(
        //    <Opaque<u8, bool>>,
        //)]
        //impl<dyn(u8) T> Custom for T {
        //    #[symbol_name = "custom_inc_move"]
        //    fn inc(self, move by: Vec<u32>) -> Self;
        //}
    }
}

#[test]
fn opaque_handles() {
    let handle: Opaque<bool, u8> = Default::default();
    let handle_ref: ExternRef<_> = handle.try_as_ref().unwrap();
    assert!(PartialEq::eq(&*handle_ref, &handle));

    let cloned = Clone::clone(&handle);
    assert!(PartialEq::eq(&handle, &cloned));

    let other: Opaque<u8, bool> = Default::default();
    let other_cloned = Clone::clone(&other);

    assert!(PartialEq::<Opaque<u8, bool>>::eq(&handle, &other));
    assert!(PartialEq::<Opaque<u8, bool>>::eq(&cloned, &other_cloned));

    let incremented = Custom::inc(handle, vec![2]);
    assert!(!PartialEq::<Opaque<u8, bool>>::eq(&incremented, &other));

    let incremented_cloned = Clone::clone(&incremented);
    assert!(PartialEq::eq(&incremented, &incremented_cloned));

    // TODO:
    //let owned_handle: Opaque<u8, bool> = Default::default();
    //let owned_incremented = Custom::inc(owned_handle, vec![2]);
    //let owned_incremented_cloned = Clone::clone(&owned_incremented);

    //assert!(PartialEq::eq(&owned_incremented, &owned_incremented_cloned));
}

