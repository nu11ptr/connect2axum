//! ProtoJSON JSON Schema for protobuf messages.
//!
//! One mapping, two ways of referring to nested messages:
//!
//! - [`SchemaBuilder::components`] makes every message a named component and
//!   refers to it with `$ref`. AsyncAPI documents use this.
//! - [`SchemaBuilder::inline_root`] inlines nested messages and puts only
//!   recursive messages under `$defs`. MCP tool input schemas use this,
//!   because clients and model APIs handle self-contained schemas best.
//!
//! The schemas describe what Buffa's generated serde code accepts. Buffa
//! messages use `#[serde(default)]` without `deny_unknown_fields`, so unknown
//! keys are ignored on input; message schemas therefore do not set
//! `additionalProperties`.

use std::collections::{BTreeMap, BTreeSet};

use flexstr::{IntoOptimizedFlexStr as _, SharedStr, ToOwnedFlexStr as _};
use serde_json::{Map, Value, json};

use crate::error::CodegenResult;
use crate::internal::api_names::{ComponentNameTracker, PackagePrefixNamer};
use crate::internal::ir::{DescriptorIr, Field, FieldKind, FieldLabel, Message};
use crate::internal::openapi::comments::comment_description;

const GOOGLE_PROTOBUF: &str = "google.protobuf.";
const NULL_VALUE: &str = "google.protobuf.NullValue";
const DEFS_POINTER: &str = "#/$defs/";

enum Refs<'a> {
    Components {
        namer: PackagePrefixNamer<'a>,
        names: ComponentNameTracker,
        pointer_prefix: &'static str,
    },
    Inline {
        recursive: BTreeMap<SharedStr, bool>,
    },
}

pub struct SchemaBuilder<'a> {
    ir: &'a DescriptorIr,
    refs: Refs<'a>,
    /// Components (component mode) or `$defs` (inline mode), keyed by the
    /// name used in `$ref`.
    definitions: Map<String, Value>,
    defined: BTreeSet<SharedStr>,
}

impl<'a> SchemaBuilder<'a> {
    /// A builder that emits one named component per message. `context` names
    /// the component kind in collision errors and `pointer_prefix` is the
    /// JSON pointer prefix for `$ref`, such as `#/components/schemas/`.
    pub fn components(
        ir: &'a DescriptorIr,
        namer: PackagePrefixNamer<'a>,
        context: &'static str,
        pointer_prefix: &'static str,
    ) -> Self {
        Self {
            ir,
            refs: Refs::Components {
                namer,
                names: ComponentNameTracker::new(context),
                pointer_prefix,
            },
            definitions: Map::new(),
            defined: BTreeSet::new(),
        }
    }

    /// Builds a self-contained schema for one message: nested messages are
    /// inlined and recursive ones are placed under `$defs`.
    pub fn inline_root(ir: &'a DescriptorIr, full_name: &str) -> CodegenResult<Value> {
        let mut builder = Self {
            ir,
            refs: Refs::Inline {
                recursive: BTreeMap::new(),
            },
            definitions: Map::new(),
            defined: BTreeSet::new(),
        };
        let mut schema = builder.message_by_name(full_name)?;
        if !builder.definitions.is_empty()
            && let Some(object) = schema.as_object_mut()
        {
            object.insert(
                "$defs".to_owned(),
                Value::Object(std::mem::take(&mut builder.definitions)),
            );
        }
        Ok(schema)
    }

    /// Ensures a component exists for a message and returns its name.
    /// Component mode only.
    pub fn component(&mut self, full_name: &str) -> CodegenResult<SharedStr> {
        let name = self.component_name(full_name)?;
        if self.defined.insert(full_name.to_owned_opt()) {
            let schema = self.message_by_name(full_name)?;
            self.definitions.insert(name.as_ref().to_owned(), schema);
        }
        Ok(name)
    }

    /// The finished component or `$defs` map.
    pub fn into_definitions(self) -> Map<String, Value> {
        self.definitions
    }

    fn component_name(&mut self, full_name: &str) -> CodegenResult<SharedStr> {
        match &mut self.refs {
            Refs::Components { namer, names, .. } => names.record(
                full_name,
                namer.component_name(full_name).into_owned().into_opt(),
            ),
            Refs::Inline { .. } => Ok(full_name.to_owned_opt()),
        }
    }

    /// The schema of a message's own body, never a `$ref`.
    fn message_by_name(&mut self, full_name: &str) -> CodegenResult<Value> {
        if let Some(schema) = well_known_schema(full_name) {
            return Ok(schema);
        }
        match self.ir.message(full_name) {
            Some(message) => self.message_schema(message),
            None => Ok(json!({ "type": "object", "additionalProperties": true })),
        }
    }

    /// The schema used where a field refers to a message.
    fn message_reference(&mut self, full_name: &str) -> CodegenResult<Value> {
        if let Some(schema) = well_known_schema(full_name) {
            return Ok(schema);
        }

        match &self.refs {
            Refs::Components { pointer_prefix, .. } => {
                let pointer_prefix = *pointer_prefix;
                let name = self.component(full_name)?;
                Ok(
                    json!({ "$ref": format!("{pointer_prefix}{}", json_pointer_escape(name.as_ref())) }),
                )
            }
            Refs::Inline { .. } => {
                if self.is_recursive(full_name) {
                    if self.defined.insert(full_name.to_owned_opt()) {
                        let schema = self.message_by_name(full_name)?;
                        self.definitions.insert(full_name.to_owned(), schema);
                    }
                    Ok(
                        json!({ "$ref": format!("{DEFS_POINTER}{}", json_pointer_escape(full_name)) }),
                    )
                } else {
                    self.message_by_name(full_name)
                }
            }
        }
    }

    fn message_schema(&mut self, message: &'a Message) -> CodegenResult<Value> {
        let mut schema = Map::new();
        schema.insert("type".to_owned(), Value::String("object".to_owned()));
        if let Some(description) = comment_description(&message.comments) {
            schema.insert("description".to_owned(), Value::String(description));
        }

        let mut properties = Map::new();
        for field in &message.fields {
            let field_schema = self.field_schema(message, field)?;
            properties.insert(field.json_name.as_ref().to_owned(), field_schema);
        }
        schema.insert("properties".to_owned(), Value::Object(properties));

        let required = message
            .fields
            .iter()
            .filter(|field| field.required)
            .map(|field| Value::String(field.json_name.as_ref().to_owned()))
            .collect::<Vec<_>>();
        if !required.is_empty() {
            schema.insert("required".to_owned(), Value::Array(required));
        }

        Ok(Value::Object(schema))
    }

    fn field_schema(&mut self, message: &Message, field: &Field) -> CodegenResult<Value> {
        let mut schema = if let Some(entry) = self.map_entry(field) {
            let (_, value) = entry
                .map_key_value()
                .expect("map entry messages have key and value fields");
            json!({
                "type": "object",
                "additionalProperties": self.single_schema(&value.kind)?
            })
        } else if field.label == Some(FieldLabel::Repeated) {
            json!({ "type": "array", "items": self.single_schema(&field.kind)? })
        } else {
            self.single_schema(&field.kind)?
        };

        let description = [
            comment_description(&field.comments),
            oneof_note(message, field),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        if !description.is_empty()
            && let Some(object) = schema.as_object_mut()
        {
            let mut description = description.join("\n\n");
            if let Some(Value::String(existing)) = object.get("description") {
                description = format!("{description}\n\n{existing}");
            }
            object.insert("description".to_owned(), Value::String(description));
        }

        Ok(schema)
    }

    fn single_schema(&mut self, kind: &FieldKind) -> CodegenResult<Value> {
        match kind {
            FieldKind::Message(full_name) | FieldKind::Group(full_name) => {
                self.message_reference(full_name.as_ref())
            }
            FieldKind::Enum(full_name) => Ok(self.enum_schema(full_name.as_ref())),
            _ => Ok(scalar_schema(kind)),
        }
    }

    fn enum_schema(&self, full_name: &str) -> Value {
        if full_name == NULL_VALUE {
            return json!({ "type": "null" });
        }
        match self.ir.enum_type(full_name) {
            Some(enum_type) if !enum_type.values.is_empty() => {
                let mut schema = json!({
                    "type": "string",
                    "enum": enum_type
                        .values
                        .iter()
                        .map(|value| value.name.as_ref())
                        .collect::<Vec<_>>()
                });
                if let Some(description) = comment_description(&enum_type.comments) {
                    schema["description"] = Value::String(description);
                }
                schema
            }
            _ => json!({ "type": "string" }),
        }
    }

    fn map_entry(&self, field: &Field) -> Option<&'a Message> {
        if field.label != Some(FieldLabel::Repeated) {
            return None;
        }
        let FieldKind::Message(full_name) = &field.kind else {
            return None;
        };
        self.ir
            .message(full_name.as_ref())
            .filter(|message| message.map_entry)
    }

    fn is_recursive(&mut self, full_name: &str) -> bool {
        let Refs::Inline { recursive } = &self.refs else {
            return false;
        };
        if let Some(known) = recursive.get(full_name) {
            return *known;
        }

        let mut visited = BTreeSet::new();
        let found = self.reaches(full_name, full_name, &mut visited);
        if let Refs::Inline { recursive } = &mut self.refs {
            recursive.insert(full_name.to_owned_opt(), found);
        }
        found
    }

    /// Whether `target` is reachable from the message fields of `from`.
    fn reaches(&self, from: &str, target: &str, visited: &mut BTreeSet<SharedStr>) -> bool {
        let Some(message) = self.ir.message(from) else {
            return false;
        };
        message_children(self.ir, message).into_iter().any(|child| {
            child.as_ref() == target
                || (visited.insert(child.clone()) && self.reaches(child.as_ref(), target, visited))
        })
    }
}

/// Message types a message's fields refer to, looking through map entries and
/// ignoring well-known types, which have fixed schemas.
fn message_children(ir: &DescriptorIr, message: &Message) -> Vec<SharedStr> {
    let mut children = Vec::new();
    for field in &message.fields {
        let (FieldKind::Message(name) | FieldKind::Group(name)) = &field.kind else {
            continue;
        };
        let name = match ir.message(name.as_ref()) {
            Some(entry) if entry.map_entry => match entry.map_key_value() {
                Some((_, value)) => match &value.kind {
                    FieldKind::Message(value_name) | FieldKind::Group(value_name) => {
                        value_name.clone()
                    }
                    _ => continue,
                },
                None => continue,
            },
            _ => name.clone(),
        };
        if well_known_schema(name.as_ref()).is_none() {
            children.push(name);
        }
    }
    children
}

fn oneof_note(message: &Message, field: &Field) -> Option<String> {
    let oneof = message.oneofs.get(field.oneof?)?;
    if oneof.synthetic {
        return None;
    }
    let members = message
        .fields
        .iter()
        .filter(|member| member.oneof == field.oneof)
        .map(|member| format!("`{}`", member.json_name.as_ref()))
        .collect::<Vec<_>>();
    let quantifier = if oneof.required {
        "Exactly one of"
    } else {
        "At most one of"
    };
    Some(format!("{quantifier}: {}.", members.join(", ")))
}

/// Fixed ProtoJSON schemas for `google.protobuf` types with special JSON
/// mappings.
fn well_known_schema(full_name: &str) -> Option<Value> {
    let name = full_name.strip_prefix(GOOGLE_PROTOBUF)?;
    let schema = match name {
        "Timestamp" => json!({ "type": "string", "format": "date-time" }),
        "Duration" => json!({
            "type": "string",
            "description": "A duration in seconds with an `s` suffix, such as \"3.5s\"."
        }),
        "FieldMask" => json!({
            "type": "string",
            "description": "Comma-separated lowerCamelCase field paths, such as \"displayName,address.city\"."
        }),
        "Struct" => json!({ "type": "object" }),
        "Value" => json!({}),
        "ListValue" => json!({ "type": "array" }),
        "Empty" => json!({ "type": "object", "properties": {} }),
        "Any" => json!({
            "type": "object",
            "properties": { "@type": { "type": "string" } },
            "required": ["@type"]
        }),
        "DoubleValue" => scalar_schema(&FieldKind::Double),
        "FloatValue" => scalar_schema(&FieldKind::Float),
        "Int64Value" => scalar_schema(&FieldKind::Int64),
        "UInt64Value" => scalar_schema(&FieldKind::Uint64),
        "Int32Value" => scalar_schema(&FieldKind::Int32),
        "UInt32Value" => scalar_schema(&FieldKind::Uint32),
        "BoolValue" => scalar_schema(&FieldKind::Bool),
        "StringValue" => scalar_schema(&FieldKind::String),
        "BytesValue" => scalar_schema(&FieldKind::Bytes),
        _ => return None,
    };
    Some(schema)
}

/// Returns true when a message's ProtoJSON form is a JSON object, which MCP
/// requires of a tool's arguments. Well-known types such as
/// `google.protobuf.StringValue` and `Timestamp` are JSON scalars.
pub fn is_json_object_message(full_name: &str) -> bool {
    well_known_schema(full_name)
        .is_none_or(|schema| schema.get("type").and_then(Value::as_str) == Some("object"))
}

/// The schema for a field kind without message or enum context.
pub fn scalar_schema(kind: &FieldKind) -> Value {
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

pub fn json_pointer_escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use flexstr::ToOwnedFlexStr as _;
    use serde_json::{Value, json};

    use super::SchemaBuilder;
    use crate::internal::api_names::PackagePrefixNamer;
    use crate::internal::ir::{
        CommentSet, DescriptorIr, EnumType, EnumValue, Field, FieldKind, FieldLabel, Message,
        Oneof, ProtoFile,
    };

    fn field(name: &str, number: i32, kind: FieldKind) -> Field {
        Field {
            name: name.to_owned_opt(),
            json_name: name.to_owned_opt(),
            number: Some(number),
            label: Some(FieldLabel::Optional),
            kind,
            ..Default::default()
        }
    }

    fn repeated(mut field: Field) -> Field {
        field.label = Some(FieldLabel::Repeated);
        field
    }

    fn message(full_name: &str, fields: Vec<Field>) -> Message {
        Message {
            name: full_name.rsplit('.').next().unwrap().to_owned_opt(),
            full_name: full_name.to_owned_opt(),
            fields,
            ..Default::default()
        }
    }

    fn ir(messages: Vec<Message>, enums: Vec<EnumType>) -> DescriptorIr {
        DescriptorIr {
            files: vec![ProtoFile {
                name: "test.proto".into(),
                package: "test.v1".into(),
                messages,
                enums,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn schema_of(fields: Vec<Field>) -> Value {
        schema_with(vec![message("test.v1.Root", fields)], Vec::new())
    }

    fn schema_with(messages: Vec<Message>, enums: Vec<EnumType>) -> Value {
        SchemaBuilder::inline_root(&ir(messages, enums), "test.v1.Root").unwrap()
    }

    fn property(schema: &Value, name: &str) -> Value {
        schema["properties"][name].clone()
    }

    #[test]
    fn maps_scalars() {
        let schema = schema_of(vec![
            field("s", 1, FieldKind::String),
            field("b", 2, FieldKind::Bool),
            field("i32", 3, FieldKind::Int32),
            field("u32", 4, FieldKind::Fixed32),
            field("i64", 5, FieldKind::Sint64),
            field("u64", 6, FieldKind::Uint64),
            field("f", 7, FieldKind::Float),
            field("d", 8, FieldKind::Double),
            field("bytes", 9, FieldKind::Bytes),
        ]);

        assert_eq!(property(&schema, "s"), json!({ "type": "string" }));
        assert_eq!(property(&schema, "b"), json!({ "type": "boolean" }));
        assert_eq!(
            property(&schema, "i32"),
            json!({ "type": "integer", "format": "int32" })
        );
        assert_eq!(
            property(&schema, "u32"),
            json!({ "type": "integer", "format": "uint32" })
        );
        assert_eq!(
            property(&schema, "i64"),
            json!({ "type": "string", "format": "int64" })
        );
        assert_eq!(
            property(&schema, "u64"),
            json!({ "type": "string", "format": "uint64" })
        );
        assert_eq!(
            property(&schema, "f"),
            json!({ "type": "number", "format": "float" })
        );
        assert_eq!(
            property(&schema, "d"),
            json!({ "type": "number", "format": "double" })
        );
        assert_eq!(
            property(&schema, "bytes"),
            json!({ "type": "string", "format": "byte" })
        );
        assert_eq!(schema["type"], "object");
        assert!(schema.get("required").is_none());
        assert!(schema.get("additionalProperties").is_none());
    }

    #[test]
    fn maps_enums_to_value_names() {
        let color = EnumType {
            name: "Color".into(),
            full_name: "test.v1.Color".into(),
            values: ["COLOR_UNSPECIFIED", "RED"]
                .into_iter()
                .enumerate()
                .map(|(number, name)| EnumValue {
                    name: name.into(),
                    number: number as i32,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let schema = schema_with(
            vec![message(
                "test.v1.Root",
                vec![
                    field("color", 1, FieldKind::Enum("test.v1.Color".into())),
                    field("missing", 2, FieldKind::Enum("other.v1.Gone".into())),
                    field(
                        "null",
                        3,
                        FieldKind::Enum("google.protobuf.NullValue".into()),
                    ),
                ],
            )],
            vec![color],
        );

        assert_eq!(
            property(&schema, "color"),
            json!({ "type": "string", "enum": ["COLOR_UNSPECIFIED", "RED"] })
        );
        assert_eq!(property(&schema, "missing"), json!({ "type": "string" }));
        assert_eq!(property(&schema, "null"), json!({ "type": "null" }));
    }

    #[test]
    fn inlines_nested_messages_and_arrays() {
        let schema = schema_with(
            vec![
                message(
                    "test.v1.Root",
                    vec![
                        field("child", 1, FieldKind::Message("test.v1.Child".into())),
                        repeated(field(
                            "children",
                            2,
                            FieldKind::Message("test.v1.Child".into()),
                        )),
                        repeated(field("tags", 3, FieldKind::String)),
                    ],
                ),
                message("test.v1.Child", vec![field("name", 1, FieldKind::String)]),
            ],
            Vec::new(),
        );

        let child = json!({ "type": "object", "properties": { "name": { "type": "string" } } });
        assert_eq!(property(&schema, "child"), child);
        assert_eq!(
            property(&schema, "children"),
            json!({ "type": "array", "items": child })
        );
        assert_eq!(
            property(&schema, "tags"),
            json!({ "type": "array", "items": { "type": "string" } })
        );
        assert!(schema.get("$defs").is_none());
    }

    #[test]
    fn maps_map_fields_to_objects() {
        let mut entry = message(
            "test.v1.Root.LabelsEntry",
            vec![
                field("key", 1, FieldKind::String),
                field("value", 2, FieldKind::Int32),
            ],
        );
        entry.map_entry = true;
        let mut root = message(
            "test.v1.Root",
            vec![repeated(field(
                "labels",
                1,
                FieldKind::Message("test.v1.Root.LabelsEntry".into()),
            ))],
        );
        root.messages = vec![entry];

        let schema = schema_with(vec![root], Vec::new());

        assert_eq!(
            property(&schema, "labels"),
            json!({
                "type": "object",
                "additionalProperties": { "type": "integer", "format": "int32" }
            })
        );
    }

    #[test]
    fn describes_oneofs_without_combinators() {
        let mut a = field("a", 1, FieldKind::String);
        a.oneof = Some(0);
        let mut b = field("b", 2, FieldKind::String);
        b.oneof = Some(0);
        b.comments = CommentSet {
            leading: Some("The b option.\n".into()),
            ..Default::default()
        };
        let mut c = field("c", 3, FieldKind::String);
        c.oneof = Some(1);
        let mut maybe = field("maybe", 4, FieldKind::String);
        maybe.proto3_optional = true;
        let mut root = message("test.v1.Root", vec![a, b, c, maybe]);
        root.oneofs = vec![
            Oneof {
                name: "choice".into(),
                ..Default::default()
            },
            Oneof {
                name: "must".into(),
                required: true,
                ..Default::default()
            },
            Oneof {
                name: "_maybe".into(),
                synthetic: true,
                ..Default::default()
            },
        ];

        let schema = schema_with(vec![root], Vec::new());

        assert_eq!(
            property(&schema, "a")["description"],
            "At most one of: `a`, `b`."
        );
        assert_eq!(
            property(&schema, "b")["description"],
            "The b option.\n\nAt most one of: `a`, `b`."
        );
        assert_eq!(
            property(&schema, "c")["description"],
            "Exactly one of: `c`."
        );
        assert_eq!(property(&schema, "maybe"), json!({ "type": "string" }));
        assert!(schema.get("oneOf").is_none());
    }

    #[test]
    fn lists_required_fields() {
        let mut name = field("name", 1, FieldKind::String);
        name.required = true;
        let mut nested = field("nested", 2, FieldKind::Message("test.v1.Child".into()));
        nested.required = true;
        let mut child_id = field("id", 1, FieldKind::String);
        child_id.required = true;

        let schema = schema_with(
            vec![
                message(
                    "test.v1.Root",
                    vec![name, nested, field("note", 3, FieldKind::String)],
                ),
                message("test.v1.Child", vec![child_id]),
            ],
            Vec::new(),
        );

        assert_eq!(schema["required"], json!(["name", "nested"]));
        assert_eq!(property(&schema, "nested")["required"], json!(["id"]));
    }

    #[test]
    fn maps_well_known_types() {
        let wkt = |name: &str, number| {
            field(
                &name.to_lowercase(),
                number,
                FieldKind::Message(format!("google.protobuf.{name}").into()),
            )
        };
        let schema = schema_of(vec![
            wkt("Timestamp", 1),
            wkt("Duration", 2),
            wkt("FieldMask", 3),
            wkt("Struct", 4),
            wkt("Value", 5),
            wkt("ListValue", 6),
            wkt("Empty", 7),
            wkt("Any", 8),
            wkt("Int64Value", 9),
            wkt("StringValue", 10),
            wkt("BoolValue", 11),
        ]);

        assert_eq!(
            property(&schema, "timestamp"),
            json!({ "type": "string", "format": "date-time" })
        );
        assert_eq!(property(&schema, "duration")["type"], "string");
        assert!(
            property(&schema, "duration")["description"]
                .as_str()
                .unwrap()
                .contains("3.5s")
        );
        assert_eq!(property(&schema, "fieldmask")["type"], "string");
        assert_eq!(property(&schema, "struct"), json!({ "type": "object" }));
        assert_eq!(property(&schema, "value"), json!({}));
        assert_eq!(property(&schema, "listvalue"), json!({ "type": "array" }));
        assert_eq!(
            property(&schema, "empty"),
            json!({ "type": "object", "properties": {} })
        );
        assert_eq!(property(&schema, "any")["required"], json!(["@type"]));
        assert_eq!(
            property(&schema, "int64value"),
            json!({ "type": "string", "format": "int64" })
        );
        assert_eq!(
            property(&schema, "stringvalue"),
            json!({ "type": "string" })
        );
        assert_eq!(property(&schema, "boolvalue"), json!({ "type": "boolean" }));
    }

    #[test]
    fn puts_recursive_messages_under_defs() {
        let schema = schema_with(
            vec![
                message(
                    "test.v1.Root",
                    vec![
                        field("tree", 1, FieldKind::Message("test.v1.Node".into())),
                        field("ping", 2, FieldKind::Message("test.v1.Ping".into())),
                    ],
                ),
                message(
                    "test.v1.Node",
                    vec![
                        field("value", 1, FieldKind::String),
                        repeated(field(
                            "children",
                            2,
                            FieldKind::Message("test.v1.Node".into()),
                        )),
                    ],
                ),
                message(
                    "test.v1.Ping",
                    vec![field("pong", 1, FieldKind::Message("test.v1.Pong".into()))],
                ),
                message(
                    "test.v1.Pong",
                    vec![field("ping", 1, FieldKind::Message("test.v1.Ping".into()))],
                ),
            ],
            Vec::new(),
        );

        assert_eq!(
            property(&schema, "tree"),
            json!({ "$ref": "#/$defs/test.v1.Node" })
        );
        assert_eq!(
            schema["$defs"]["test.v1.Node"]["properties"]["children"]["items"],
            json!({ "$ref": "#/$defs/test.v1.Node" })
        );
        assert_eq!(
            property(&schema, "ping"),
            json!({ "$ref": "#/$defs/test.v1.Ping" })
        );
        assert_eq!(
            schema["$defs"]["test.v1.Ping"]["properties"]["pong"],
            json!({ "$ref": "#/$defs/test.v1.Pong" })
        );
        assert!(schema["$defs"].get("test.v1.Pong").is_some());
    }

    #[test]
    fn recursive_root_is_inlined_and_defined() {
        let schema = schema_with(
            vec![message(
                "test.v1.Root",
                vec![field("next", 1, FieldKind::Message("test.v1.Root".into()))],
            )],
            Vec::new(),
        );

        assert_eq!(schema["type"], "object");
        assert_eq!(
            property(&schema, "next"),
            json!({ "$ref": "#/$defs/test.v1.Root" })
        );
        assert!(schema["$defs"].get("test.v1.Root").is_some());
    }

    #[test]
    fn components_mode_references_every_message() {
        let ir = DescriptorIr {
            files: vec![
                ProtoFile {
                    name: "one.proto".into(),
                    package: "one.v1".into(),
                    messages: vec![message(
                        "one.v1.Root",
                        vec![field("other", 1, FieldKind::Message("two.v1.Other".into()))],
                    )],
                    ..Default::default()
                },
                ProtoFile {
                    name: "two.proto".into(),
                    package: "two.v1".into(),
                    messages: vec![message(
                        "two.v1.Other",
                        vec![field(
                            "at",
                            1,
                            FieldKind::Message("google.protobuf.Timestamp".into()),
                        )],
                    )],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut builder = SchemaBuilder::components(
            &ir,
            PackagePrefixNamer::new(&ir, true),
            "test component",
            "#/components/schemas/",
        );

        assert_eq!(builder.component("one.v1.Root").unwrap().as_ref(), "Root");
        let definitions = builder.into_definitions();

        assert_eq!(
            definitions["Root"]["properties"]["other"],
            json!({ "$ref": "#/components/schemas/Other" })
        );
        assert_eq!(
            definitions["Other"]["properties"]["at"],
            json!({ "type": "string", "format": "date-time" })
        );
        assert_eq!(definitions.len(), 2);
    }
}
