use serde_json::{Map, Value, json};

use crate::ir::{FieldKind, FieldLabel};
use crate::shape::ShapeField;

use super::comments::comment_description;

pub(crate) fn generated_dto_schema(fields: &[ShapeField]) -> Value {
    let properties = fields
        .iter()
        .map(|field| {
            (
                field.field.json_name.as_ref().to_owned(),
                field_schema(field),
            )
        })
        .collect::<Map<_, _>>();

    json!({
        "type": "object",
        "properties": properties
    })
}

pub(crate) fn field_schema(field: &ShapeField) -> Value {
    let mut schema = if field.field.label == Some(FieldLabel::Repeated) {
        json!({
            "type": "array",
            "items": scalar_field_schema(&field.field.kind)
        })
    } else {
        scalar_field_schema(&field.field.kind)
    };

    if let Some(description) = comment_description(&field.field.comments)
        && let Some(schema) = schema.as_object_mut()
    {
        schema.insert("description".to_owned(), Value::String(description));
    }

    schema
}

pub(crate) fn scalar_field_schema(kind: &FieldKind) -> Value {
    match kind {
        FieldKind::Double => json!({ "type": "number", "format": "double" }),
        FieldKind::Float => json!({ "type": "number", "format": "float" }),
        FieldKind::Int64 | FieldKind::Sint64 | FieldKind::Sfixed64 => {
            json!({ "type": "string", "format": "int64" })
        }
        FieldKind::Uint64 | FieldKind::Fixed64 => json!({ "type": "string", "format": "uint64" }),
        FieldKind::Int32 | FieldKind::Sint32 | FieldKind::Sfixed32 => {
            json!({ "type": "integer", "format": "int32" })
        }
        FieldKind::Uint32 | FieldKind::Fixed32 => json!({ "type": "integer", "format": "uint32" }),
        FieldKind::Bool => json!({ "type": "boolean" }),
        FieldKind::String => json!({ "type": "string" }),
        FieldKind::Bytes => json!({ "type": "string", "format": "byte" }),
        FieldKind::Enum(_) => json!({ "type": "string" }),
        FieldKind::Group(_) | FieldKind::Message(_) | FieldKind::Unknown => {
            json!({ "type": "object", "additionalProperties": true })
        }
    }
}
