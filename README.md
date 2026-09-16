# CO3

[<img alt="crates.io" src="https://img.shields.io/crates/v/co3.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/co3)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-co3-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/co3)
[<img alt="CI" src="https://img.shields.io/github/actions/workflow/status/mversic/co3/main.yaml?style=for-the-badge&label=CI" height="20">](https://github.com/mversic/co3/actions/workflows/main.yaml)

_Safely_ export and/or import FFI bindings:

1. **Native-Rust ergonomics**
- ergonomics of APIs and generated wrappers is idiomatic to Rust users.
- FFI boundary mechanics are zero-cost abstracted yet remain configurable.
- The syntax of exports and imports is completely interchangeable.

2. **Soundness-first FFI interoperability**
- soundness is never weakened for the sake of performance or memory footprint in the default configuration.
- if preserving soundness requires additional validation, temporary storage, or cloning, that cost is accepted.
- only explicit opt-in modes prioritize performance by explicitly shifting soundness responsibility to the user.

# Example

Using `CO3` is super-duper simple yet highly expressive. In a nutshell:
- mark Rust types that cross the boundary with `#[derive(ReprC)]`
- describe the boundary API with `ffi!` (fns, impls and types)

```rust
use co3::{ffi, rust_spec::RustSpec, ReprC};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
struct Vec2 {
    x: f32,
    y: f32,
}

struct Camera {
    position: Vec2,
}

impl Camera {
    fn translate(&mut self, by: Vec2) {
        self.position.x += by.x;
        self.position.y += by.y;
    }

    fn position(&self) -> Vec2 {
        self.position
    }
}

fn distance(from: Vec2, to: Vec2) -> f32 {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    (dx * dx + dy * dy).sqrt()
}

ffi! {
    // Declares exports of existing Rust items (fns, impls, types)
    // Change to `#![unsafe(extern("C"))]` to declare imports
    #![unsafe(export("C"))]

    // Define the prefix of FFI symbols
    // The default is `CARGO_CRATE_NAME`
    #![symbol_prefix = "geometry"]

    // Opaque type
    type Camera;

    impl Camera {
        // Exported as `translate`
        #[symbol_name = "translate"]
        fn translate(&mut self, by: Vec2);

        // Exported as `geometry__Camera__position`
        fn position(&self) -> Vec2;
    }

    // Exported as `geometry__distance`
    fn distance(from: Vec2, to: Vec2) -> f32;
}
```

Note that type deriving `ReprC`, although recommended, is not required to have a stable representation.

## Tagged Dispatch

It is common in FFI for several concrete types to share one C representation. Think of FFI functions
like [SQLAllocHandle](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlallochandle-function)
which works for different tag types
or [`SQLSetEnvAttr`](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlsetenvattr-function)
where an attribute's concrete type determines the accepted value representation.

```rust
use co3::{ffi, rust_spec::RustSpec, Tag, ReprC};

trait Calibrate {
    fn calibrate(&mut self, by: u16);
}

#[derive(RustSpec, Tag, ReprC)]
#[tag(u8, unsafe(1))]
#[repr(transparent)]
struct Celsius(u16);

#[derive(RustSpec, Tag, ReprC)]
#[tag(u8, unsafe(2))]
#[repr(transparent)]
struct Fahrenheit(u16);

impl Calibrate for Celsius {
    fn calibrate(&mut self, by: u16) {
        self.0 += by;
    }
}

impl Calibrate for Fahrenheit {
    fn calibrate(&mut self, by: u16) {
        self.0 += by;
    }
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "thermometer"]

    // - T is declared as tag-dispatched
    // - u8 is the type of the tag argument
    // - every concrete `T` is erased as `u16`
    impl<dyn(u8) T = u16> Calibrate for T
    where
        // `T` is instantiated as either
        use<T> @ (<Celsius> | <Fahrenheit>),
    {
        fn calibrate(&mut self, by: u16);
    }
}
```

# ABI Stability

Although this crate is pre-1.0.0, its ABI is considered stable. This does not mean the API is stable. In practice:
- ABI stability means FFI contracts (symbol names, calling conventions, and data layout expectations) are intended to remain compatible across updates.
- API instability means Rust-facing items (fns, trait shapes, modules, and type signatures) may still change and require source updates when upgrading.

In other words, external binaries that integrate through the defined ABI should keep working, while Rust code using this crate directly may need refactoring between releases.
