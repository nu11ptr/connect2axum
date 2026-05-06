---
name: flexstr
description: connect2axum Rust string conventions. Use when editing Rust code in this repo, especially when choosing owned string types, cloning strings, storing parser/codegen names, or reviewing code that uses String, Arc<str>, SharedStr, or LocalStr.
---

# FlexStr

Prefer `flexstr` owned string types over `String` when the value is stored,
cloned, passed around, or used as semantic data.

Use `String` while code is actively constructing or mutating text, especially
generated source buffers, formatted output, accumulated diagnostics, or external
APIs that require `String`. Once construction is complete, prefer converting the
finished value into optimized `SharedStr`/`LocalStr` before storing it if the
value will be cloned, retained, or carried through project data structures.

## Types

- `SharedStr`: main/default owned string type because it is `Send` + `Sync`;
  use it for public structs, long-lived values, cross-thread values, maps, and
  cloned names.
- `LocalStr`: faster owned string type, but not `Send` or `Sync`; use it only
  when the value is definitely local to one thread and will not cross task,
  worker, public API, or shared-state boundaries.
- `String`: use for active mutable construction, generated file contents,
  formatting buffers, or unavoidable external API boundaries. Do not keep
  finished semantic values as `String` just because they were built as `String`.

## Imports

```rust
use flexstr::str::SharedStrRef;
use flexstr::{IntoOptimizedFlexStr as _, SharedStr, ToOwnedFlexStr as _};
```

## Creation Patterns

- From `&str`: use `s.to_owned_opt()` for an optimized owned copy.
- From literals: use `"literal".into()`.
- From consumed `String`: use `s.into_opt()` when storing the finished value.
- From formatted or incrementally built text: keep `String` while constructing,
  then convert with `.into_opt()` at the storage boundary when the result becomes
  semantic data. This preserves efficient clones for values like file names,
  generated identifiers, package names, resolved paths, and option values.

Keep external `String` requirements at the boundary and do not let them leak
into project data structures by default.
