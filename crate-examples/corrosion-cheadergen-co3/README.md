# corrosion + cheadergen + co3

A C program calling a Rust function exported with `ffi!`, with the C header generated from the
`ffi!` declarations rather than written by hand.

Three tools, one per step:

- [`co3`](../..) declares the boundary and generates the `extern "C"` definitions.
- [`cheadergen`](https://cheadergen.com) derives `corrosion_cheadergen_co3.h` from those
  definitions. It reads `rustdoc` JSON, so the C types it emits are the ones the compiler
  resolved, including associated types such as `<T as co3::ExternC>::CType`.
- [Corrosion](https://github.com/corrosion-rs/corrosion) builds the crate as a static library and
  links it into the C executable.

## Prerequisites

`cheadergen` on `PATH`, plus the nightly toolchain it pins for `rustdoc` JSON:

```console
$ cargo install --locked cheadergen_cli
$ rustup install nightly-2026-06-04 -c rust-docs-json
```

CMake fetches Corrosion itself, so the first configure needs network access.

## Build and run

```console
$ cmake -S . -B build
$ cmake --build build
$ ./build/demo
rust_super_safe_add(2, 40) = 42
```

`ctest --test-dir build` runs the same binary as a test.

## What to look at

`src/lib.rs` exports one function under a custom symbol name:

```rust
co3::ffi! {
    #![unsafe(export("C"))]

    #[symbol_name = "rust_super_safe_add"]
    pub fn add(left: u64, right: u64) -> u64;
}
```

`build/include/corrosion_cheadergen_co3.h` is generated from it:

```c
uint64_t rust_super_safe_add(uint64_t left, uint64_t right);
```

Nothing declares that signature in C; it is derived from the Rust declaration, so the two cannot
drift apart. If `ffi!` stopped emitting a discoverable definition the header would come out empty
and `main.c` would fail to compile, which makes this example a build-level regression test.

## Limitation

`cheadergen` resolves an associated type by finding a concrete `impl` in the rustdoc JSON. `co3`
maps references through generic impls (`impl<R: ExternC + ?Sized> ExternC for &R`), which it
cannot see through, so methods taking `&self` or `&mut self` are reported as unresolved and
omitted from the header. This example stays on by-value arguments for that reason.
