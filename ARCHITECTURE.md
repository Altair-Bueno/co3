# Architecture

## 1. Mental Model

Conversion is built from four layers:

1. **Family classification** (`ReprFamily` and related traits): decide type-level properties used by conversion.
2. **ABI mapping** (`ExternC`): map the Rust type into a robust `repr(C)` type based on the family classification.
3. **Value conversion** (`Encode`/`Decode`): convert values to and from that robust `repr(C)` type.
4. **Post-call writeback** (`Store::sync`): apply deferred updates for mutable reference paths.

Family classification is defined in [`co3-types/ARCHITECTURE.md`](co3-types/ARCHITECTURE.md).
