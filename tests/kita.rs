//use co3::ffi;
//
//trait ExternType {}
//trait Supported {}
//
//struct BoundedTy<T>(T);
//struct Item(u8);
//
//impl Supported for u8 {}
//
//fn selected<T: Supported>(value: T) {
//    let _ = value;
//}
//
//impl Item {
//    fn selected<T: Supported>(value: T) {
//        let _ = value;
//    }
//}
//
//ffi! {
//    #![unsafe(export("C"))]
//
//    #[unsafe(id(u32 = 0))]
//    type BoundedTy<T>;
//}
//
