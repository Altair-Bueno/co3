


# ABI Stability

Although this crate is pre-1.0.0, its ABI is considered stable. This does not mean the API is stable.

In practice:
- ABI stability means FFI contracts (exported/imported symbols, calling conventions, and data layout expectations) are intended to remain compatible across updates.
- API instability means Rust-facing items (function names, trait shapes, modules, and type signatures) may still change and require source updates when upgrading.

In other words, external binaries that integrate through the defined ABI should keep working, while Rust code using this crate directly may need refactoring between releases.
