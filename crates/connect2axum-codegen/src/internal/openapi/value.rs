use serde_json::{Map, Value};
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};

pub fn ensure_object_at<'a>(root: &'a mut Value, field: &str) -> &'a mut Map<String, Value> {
    root.as_object_mut()
        .expect("OpenAPI document root should be an object")
        .entry(field.to_owned())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("OpenAPI root field should be an object")
}

pub fn ensure_nested_object<'a>(
    root: &'a mut Map<String, Value>,
    field: &str,
) -> CodegenResult<&'a mut Map<String, Value>> {
    root.entry(field.to_owned())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiInvalidDocument,
                format!("OpenAPI field {field} was not an object"),
            )
        })
}

pub fn ensure_array_at<'a>(root: &'a mut Value, field: &str) -> CodegenResult<&'a mut Vec<Value>> {
    root.as_object_mut()
        .expect("OpenAPI document root should be an object")
        .entry(field.to_owned())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiInvalidDocument,
                format!("OpenAPI field {field} was not an array"),
            )
        })
}

pub fn merge_named_values(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
    context: &str,
) -> CodegenResult<()> {
    for (key, value) in source {
        match target.get(key) {
            Some(existing) if existing != value => {
                return Err(UniError::from_kind_context(
                    CodegenErrKind::OpenApiMergeConflict,
                    format!("conflicting OpenAPI key {key:?} while merging {context}"),
                ));
            }
            Some(_) => {}
            None => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
    Ok(())
}
