use co3::{Handle, ffi};

#[derive(Handle)]
#[handle(unsafe(id(u8 = 1)))]
enum First {}

#[derive(Handle)]
#[handle(unsafe(id(u8 = 2)))]
enum Open {}

ffi! {
    #![unsafe(extern("C"))]

    pub fn fully_open<dyn(u8) T>(tag: <dyn T>::ID);

    pub fn mixed<dyn(u8) T, dyn(u8) U>(open_id: <dyn U>::ID)
    where
        use<T> @ <First>;
}

fn assert_open_dispatch_set<T>()
where
    (): fully_open::DispatchSet<T>,
{
}

fn assert_mixed_dispatch_set<U>()
where
    (): mixed::DispatchSet<First, U>,
{
}

fn main() {
    assert_open_dispatch_set::<First>();
    assert_mixed_dispatch_set::<Open>();
}
