# Specification

## 1. Design Goal

`co3` is a Rust-side framework for _safely_ exporting/importing C ABI functions by:

- Mapping Rust types to C-compatible types via `ReprC` derive macro.
- Writing export/extern FFI declarations via `ffi!` fn-like macro.

### 1.1 Hard Guarantees

These guarantees define the contract of this library:

1. **Native-Rust ergonomics**
- Ergonomics of APIs and generated wrappers **MUST** remain idiomatic to Rust users.
- FFI boundary mechanics **SHOULD** stay encapsulated in conversion traits and generated glue.

2. **Soundness-first FFI interoperability**
- Soundness **MUST NOT** be weakened for performance or memory footprint in the default configuration.
- If preserving soundness requires additional validation, temporary storage, or cloning, that cost is accepted.

3. **Zero-cost abstraction by default**
- Zero-cost abstraction **MUST** be preserved unless it directly conflicts with the soundness guarantee.
- Only explicit opt-in modes **MAY** prioritize performance by shifting soundness responsibility to the user.

### 1.2 FFI Conversion Modes

Conversion modes define how values cross the FFI boundary, including ownership behavior, pointer-identity semantics, and validation strictness.
Each mode makes explicit tradeoffs and is selected through compile-time configuration:

1. **`move` (opt-in, on fn arguments or as `move fn`)**
- `Drop` types are borrowed instead of transferring ownership unless argument/return is `move`d
- Owned backing storage is kept alive in a type-specific store while exposing borrowed FFI views.
- `move` removes the cost of cloning owned types in decode paths that is incurred by default.

2. **`#[soft]` (opt-in, on fn arguments)**
- Enables additional reference conversion paths that rely on cloning the referent (e.g. `&(u8,)`).
- Uses intermediate owned/cloned values and store synchronization for mutable writeback paths.
- Pointer identity is not preserved and pointer equality for these types **MUST NOT** be relied on.

3. **Tagged dispatch (opt-in, on impl blocks, free functions, and inherent methods)**
- Enables tagged generic dispatch where type's C-compatible representation is erased into a shared type and reinterpreted back via the tag value.
- Dispatched generics are defined by `<dyn({TagTy}) T = {ErasedTy}>` where `ErasedTy::CType` constrains size and alignment of erased types.
- A `use<T, ...> @ (<Param1> | ...)` where predicate declares the concrete types dispatched that have a tag value assigned (best via `handles!`).

## 2. Public API

Public API constitutes user-facing macros only (not the public trait and type exports).
Any generated glue code **MUST** remain private and **MUST NOT** leak into the public API.
Any conversion written manually against traits of this crate **DOES NOT** constitute public API.

### 2.1 `ReprC` Derive Macro

`#[derive(ReprC)]` derives implementations required to convert a type to a corresponding generated C-compatible companion type.
A C-compatible companion type is a type with a defined C ABI and no trap representations, whose fields are themselves C-compatible companion types.

- By default, the derive defines a C-compatible companion type and conversions between the two types.
- Conversion of types with explicit representation (i.e. `#[repr(C)]`/`repr(transmute)`) are optimized.
- `#[reprC(is_valid = |field0, ...| {...})]` provides additional validity invariant of a struct/variant.
- `#[reprC(NICHE_VALUE = <expr>)]` defines the struct's trap value that is used for niche optimization.
- `#[reprC(id(TagTy))]` defines the tag type that identifies the item when it is erased by dynamic dispatch.

### 2.2 The `ffi!` Macro

`ffi!` is a fn-like macro that enables writing export/extern declarations of types, methods and impl blocks.
It must always start with a declaration of direction and ABI (e.g. `#![unsafe(export("system"))]`/`#![unsafe(extern("system"))]`).

- `#![unsafe(export("ABI"))]` creates export declarations with the given ABI. The declared items must exist and be resolvable.
- `#![unsafe(extern("ABI"))]` creates import declarations with the given ABI. The macro is said to contain extern declarations.
- `#![feature(extern_types)]` opts into the corresponding unstable macro codegen path; no other feature names are supported.
- Trait method symbol names are inferred as `{symbol_prefix}__{TraitPath}__{SelfTy}__{method}`.
- Inherent method symbol names are inferred as `{symbol_prefix}__{SelfTy}__{method}`.
- Free function symbol names are inferred as `{symbol_prefix}__{fn_name}`.
- `#![symbol_prefix = "..."]` defines the symbol prefix (defaults to `CARGO_CRATE_NAME`).
- `#[symbol_name = "..."]` overrides the name mangling enforced by the `ffi` macro.
- `#![failure = "panic" | "error"]` controls whether internal failures panic(default) or are returned.
- `type Type;` declares an opaque type (it's representation is unknown). This type should not be dereferenced.
- `#[id(TagTy)]` on a type declaration defines the tag type that identifies the type when it is erased by dynamic dispatch.
- `where use<T> @ (<Type1> | ...)` opts into a kind of polymorphic dispatch where concrete types are known at compile time but erased at runtime.
- `..` splits a wide companion type into separate data and metadata arguments at the ABI boundary.
- `#[explicit_lifetimes]` opts into declarations with explicit lifetimes inside `ffi`.
- `cfg_attr` is fully supported in all attribute positions inside the `ffi` macro.
- Although not declared `unsafe`, using `ffi` macro always carries a risk of UB.
