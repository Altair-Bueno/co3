use co3::{ffi, handles, handle::HandleFamily};

trait Trait {}
struct Kita;

pub struct GenericHandle<'a, T, const N: usize>(&'a [T; N]);

impl HandleFamily for Kita {
    type Kind = u32;
}

handles! {
    GenericHandle<'_, u32, 23>,
    Kita,
}

ffi! {
    #![export("C")]

    #[id(u32)]
    pub type GenericHandle<'a, T, const N: usize>;

    #[unsafe(lifetimes)]
    #[dispatch(<Kita, 23>)]
    impl<'a, dyn(u32) U, const K: usize> Drop for GenericHandle<'a, U, K> {
        fn drop(#[soft] move &mut self);
    }
}

impl GenericHandle<'static, u32, 12> {
    pub fn export1<'a>(&self) {}
}

impl<'a> GenericHandle<'a, u32, 12> {
    pub fn export2(&self) {}
}

impl<T> GenericHandle<'static, T, 12> {
    pub fn export3(&self) {}
}

impl<const N: usize> GenericHandle<'static, u32, N> {
    pub fn handle3(&self) {}
}

pub extern "C" fn export1<'a>(v: &'a u32) -> &'a u32 {
    v
}

pub extern "C" fn export2<T>(v: T) -> T {
    v
}

pub extern "C" fn export3<const N: usize>(v: [u32; N]) -> [u32; N] {
    v
}

ffi! {
    #![export("C")]

    impl GenericHandle<'static, u32, 12> {
        #[unsafe(lifetimes)]
        pub fn export1<'a>(&self);
    }

    #[unsafe(lifetimes)]
    impl<'a> GenericHandle<'a, u32, 12> {
        pub fn export2(#[by_val] &self);
    }

    impl<T> GenericHandle<'static, T, 12> {
        pub fn export3(&self);
    }

    impl<const N: usize> GenericHandle<'static, u32, N> {
        pub fn handle3(&self);
    }

    #[unsafe(lifetimes)]
    pub extern "C" fn export1<'a>(v: &'a u32) -> &'a u32;
    pub extern "C" fn export2<T>(v: T) -> T;
    pub extern "C" fn export3<const N: usize>(v: [u32; N]) -> [u32; N];

    #[unsafe(lifetimes)]
    #[dispatch(<Kita, 23>)]
    impl<'a, dyn(u32) U, const K: usize> Trait for GenericHandle<'a, U, K> {}
}

ffi! {
    #![extern("C")]

    #![symbol_prefix = "kita"]

    impl GenericHandle<'static, u32, 12> {
        #[unsafe(lifetimes)]
        pub fn extern1<'a>(&self);
    }
    impl GenericHandle<'_, u32, 12> {
        pub fn extern2(&self);
    }
    impl<T> GenericHandle<'static, T, 12> {
        pub fn extern3(&self);
    }
    impl<const N: usize> GenericHandle<'static, u32, N> {
        pub fn handle3(&self);
    }

    #[unsafe(lifetimes)]
    pub extern "C" fn extern1<'a>(v: &'a u32) -> &'a u32;
    pub extern "C" fn extern2<T>(v: T) -> T;
    pub extern "C" fn extern3<const N: usize>(v: [u32; N]) -> [u32; N];

    #[unsafe(lifetimes)]
    #[dispatch(<Kita, 23>)]
    impl<'a, dyn(u32) U, const K: usize> Trait for GenericHandle<'a, U, K> {
        fn drop(self_id: <dyn U>::ID, &mut self);
    }
}

fn main() {}
