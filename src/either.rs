//! Anonymous sum types used by generated glue.

use crate::Store;

macro_rules! impl_either {
    (($( $ty:ident : $variant:ident ),+) -> $either:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[doc(hidden)]
        pub enum $either<$($ty),+> {
            $($variant($ty)),+
        }

        impl<$($ty: Store),+> Store for $either<$($ty),+> {
            fn sync(self) -> Option<()> {
                match self {
                    $(Self::$variant(value) => value.sync()),+
                }
            }
        }
    };
}

impl_either!((A: V0) -> Either1);
impl_either!((A: V0, B: V1) -> Either2);
impl_either!((A: V0, B: V1, C: V2) -> Either3);
impl_either!((A: V0, B: V1, C: V2, D: V3) -> Either4);
impl_either!((A: V0, B: V1, C: V2, D: V3, E: V4) -> Either5);
impl_either!((A: V0, B: V1, C: V2, D: V3, E: V4, F: V5) -> Either6);
impl_either!((A: V0, B: V1, C: V2, D: V3, E: V4, F: V5, G: V6) -> Either7);
impl_either!((A: V0, B: V1, C: V2, D: V3, E: V4, F: V5, G: V6, H: V7) -> Either8);
impl_either!((A: V0, B: V1, C: V2, D: V3, E: V4, F: V5, G: V6, H: V7, I: V8) -> Either9);
impl_either!((A: V0, B: V1, C: V2, D: V3, E: V4, F: V5, G: V6, H: V7, I: V8, J: V9) -> Either10);
impl_either!((A: V0, B: V1, C: V2, D: V3, E: V4, F: V5, G: V6, H: V7, I: V8, J: V9, K: V10) -> Either11);
impl_either!((A: V0, B: V1, C: V2, D: V3, E: V4, F: V5, G: V6, H: V7, I: V8, J: V9, K: V10, L: V11) -> Either12);
