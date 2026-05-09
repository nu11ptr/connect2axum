use std::collections::HashMap;

use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};

pub(crate) fn ensure_unique_generated_identifiers(
    scope: &str,
    values: impl IntoIterator<Item = (String, String)>,
) -> CodegenResult<()> {
    ensure_unique(
        CodegenErrKind::DuplicateGeneratedIdentifier,
        "generated Rust identifier",
        scope,
        values,
    )
}

pub(crate) fn ensure_unique_routes(
    scope: &str,
    values: impl IntoIterator<Item = (String, String)>,
) -> CodegenResult<()> {
    ensure_unique(CodegenErrKind::DuplicateRoute, "route", scope, values)
}

fn ensure_unique(
    kind: CodegenErrKind,
    value_kind: &str,
    scope: &str,
    values: impl IntoIterator<Item = (String, String)>,
) -> CodegenResult<()> {
    let mut seen = HashMap::new();

    for (value, context) in values {
        if let Some(previous) = seen.insert(value.clone(), context.clone()) {
            return Err(UniError::from_kind_context(
                kind,
                format!("duplicate {value_kind} {value:?} in {scope}: {previous}; {context}"),
            ));
        }
    }

    Ok(())
}
