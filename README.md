# CO3

[<img alt="crates.io" src="https://img.shields.io/crates/v/co3.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/co3)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-co3-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/co3)
[<img alt="CI" src="https://img.shields.io/github/actions/workflow/status/mversic/co3/main.yaml?style=for-the-badge&label=CI" height="20">](https://github.com/mversic/co3/actions/workflows/main.yaml)

# ABI Stability

Although this crate is pre-1.0.0, its ABI is considered stable. This does not mean the API is stable.

In practice:
- ABI stability means FFI contracts (exported/imported symbols, calling conventions, and data layout expectations) are intended to remain compatible across updates.
- API instability means Rust-facing items (function names, trait shapes, modules, and type signatures) may still change and require source updates when upgrading.

In other words, external binaries that integrate through the defined ABI should keep working, while Rust code using this crate directly may need refactoring between releases.
