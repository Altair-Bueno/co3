use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    type MethodType;

    impl dyn MethodType {
        fn method(&self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    type DropType;

    impl Drop for dyn DropType {
        fn drop(&mut self);
    }
}

fn main() {}
