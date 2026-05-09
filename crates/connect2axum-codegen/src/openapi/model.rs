use serde_json::Value;
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};

pub(crate) fn validate_document(document: &Value) -> CodegenResult<()> {
    let content = serde_json::to_string(document).map_err(|err| {
        UniError::from_kind_context(
            CodegenErrKind::OpenApiInvalidDocument,
            format!("failed to serialize OpenAPI document for validation: {err}"),
        )
    })?;

    let spec = oas3::from_json(&content).map_err(|err| {
        UniError::from_kind_context(
            CodegenErrKind::OpenApiInvalidDocument,
            format!("OpenAPI document failed oas3 validation parse: {err}"),
        )
    })?;

    // Exercise the typed model enough to catch malformed path/operation shapes.
    let _operation_count = spec.operations().count();

    Ok(())
}
