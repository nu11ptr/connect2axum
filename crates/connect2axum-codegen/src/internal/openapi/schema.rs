use serde_json::{Map, Value, json};

use crate::internal::ir::FieldLabel;
use crate::internal::schema::scalar_schema;
use crate::internal::shape::ShapeField;

use super::comments::comment_description;

pub fn generated_dto_schema(fields: &[ShapeField]) -> Value {
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

pub fn field_schema(field: &ShapeField) -> Value {
    let mut schema = if field.field.label == Some(FieldLabel::Repeated) {
        json!({
            "type": "array",
            "items": scalar_schema(&field.field.kind)
        })
    } else {
        scalar_schema(&field.field.kind)
    };

    if let Some(description) = comment_description(&field.field.comments)
        && let Some(schema) = schema.as_object_mut()
    {
        schema.insert("description".to_owned(), Value::String(description));
    }

    schema
}
