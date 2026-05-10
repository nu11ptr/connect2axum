use std::collections::HashMap;
use std::collections::hash_map::Entry;

use flexstr::SharedStr;

use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};

pub fn ensure_unique_generated_identifiers(
    scope: &str,
    values: impl IntoIterator<Item = (SharedStr, String)>,
) -> CodegenResult<()> {
    ensure_unique(
        CodegenErrKind::DuplicateGeneratedIdentifier,
        "generated Rust identifier",
        scope,
        values,
    )
}

pub fn ensure_unique_routes(
    scope: &str,
    values: impl IntoIterator<Item = (SharedStr, String)>,
) -> CodegenResult<()> {
    ensure_unique(CodegenErrKind::DuplicateRoute, "route", scope, values)
}

fn ensure_unique(
    kind: CodegenErrKind,
    value_kind: &str,
    scope: &str,
    values: impl IntoIterator<Item = (SharedStr, String)>,
) -> CodegenResult<()> {
    let mut seen = HashMap::new();

    for (value, context) in values {
        match seen.entry(value) {
            Entry::Vacant(entry) => {
                entry.insert(context);
            }
            Entry::Occupied(entry) => {
                return Err(UniError::from_kind_context(
                    kind,
                    format!(
                        "duplicate {value_kind} {:?} in {scope}: {}; {context}",
                        entry.key().as_ref(),
                        entry.get()
                    ),
                ));
            }
        }
    }

    Ok(())
}
