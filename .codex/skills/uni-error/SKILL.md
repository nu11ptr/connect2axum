---
name: uni-error
description: connect2axum uni_error conventions. Use when adding or modifying fallible Rust code, compiler-plugin errors, error kinds, HTTP/Connect error mapping, or replacing anyhow, thiserror, boxed errors, or ad hoc string errors in this repo.
---

# uni_error

Use `uni_error` for project error handling. Do not introduce direct `anyhow` or
`thiserror` dependencies unless an external integration gives us no practical
alternative.

Use `DynResult<T>` only at top-level binary or plugin boundaries. For library
code, define a small `XxxErrKind` enum and return `UniResult<T, XxxErrKind>`.

## Imports

```rust
use uni_error::{Cause, ResultContext as _, UniError, UniKind, UniResult};
use uni_error::{DynResult, UniResultContext as _};
```

## Error Kind Pattern

```rust
#[derive(Debug)]
pub enum CodegenErrKind {
    InvalidPluginOption,
    UnsupportedHttpRule,
}

impl UniKind for CodegenErrKind {
    fn context(&self, _cause: Option<Cause<'_>>) -> Option<std::borrow::Cow<'static, str>> {
        match self {
            CodegenErrKind::InvalidPluginOption => Some("invalid connect2axum plugin option".into()),
            CodegenErrKind::UnsupportedHttpRule => Some("unsupported google.api.http rule".into()),
        }
    }
}
```

## Context Methods

- Use `UniError::from_kind(...)` for direct errors.
- Use `UniError::from_kind_context(...)` for direct errors with context.
- Use `.kind_context(...)` for wrapping standard `Result`/`Option` values.
- Use `.kind_into_context(...)` when converting `UniResult<T, K1>` to
  `UniResult<T, K2>` and `From<K1> for K2` exists.

## Project Guidance

- Codegen library internals should prefer typed `UniResult`.
- The `protoc-gen-connect2axum` binary can return `DynResult<()>`.
- Generated REST/runtime error mapping should preserve structured error kinds
  rather than flattening everything to text.
