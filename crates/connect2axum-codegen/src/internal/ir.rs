use std::collections::HashMap;

use connectrpc_codegen::codegen::descriptor::{
    self, DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorProto,
    MethodDescriptorProto, ServiceDescriptorProto,
};
use flexstr::{SharedStr, ToOwnedFlexStr as _};
use uni_error::{ResultContext as _, UniError};

use crate::CodeGeneratorRequest;
use crate::error::{CodegenErrKind, CodegenResult};
use crate::internal::{field_options, http};

const FILE_MESSAGE_TYPE_PATH: i32 = 4;
const FILE_ENUM_TYPE_PATH: i32 = 5;
const FILE_SERVICE_PATH: i32 = 6;
const MESSAGE_FIELD_PATH: i32 = 2;
const MESSAGE_NESTED_TYPE_PATH: i32 = 3;
const MESSAGE_ENUM_TYPE_PATH: i32 = 4;
const ENUM_VALUE_PATH: i32 = 2;
const SERVICE_METHOD_PATH: i32 = 2;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DescriptorIr {
    pub files: Vec<ProtoFile>,
    pub descriptor_files: Vec<FileDescriptorProto>,
    pub files_to_generate: Vec<SharedStr>,
}

impl DescriptorIr {
    pub fn file(&self, name: &str) -> Option<&ProtoFile> {
        self.files.iter().find(|file| file.name.as_ref() == name)
    }

    pub fn message(&self, full_name: &str) -> Option<&Message> {
        self.files
            .iter()
            .flat_map(|file| file.messages.iter())
            .find_map(|message| find_message(message, full_name))
    }
    pub fn enum_type(&self, full_name: &str) -> Option<&EnumType> {
        self.files.iter().find_map(|file| {
            file.enums
                .iter()
                .find(|enum_type| enum_type.full_name.as_ref() == full_name)
                .or_else(|| {
                    file.messages
                        .iter()
                        .find_map(|message| find_nested_enum(message, full_name))
                })
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProtoFile {
    pub name: SharedStr,
    pub package: SharedStr,
    pub messages: Vec<Message>,
    pub enums: Vec<EnumType>,
    pub services: Vec<Service>,
}

impl ProtoFile {
    pub fn has_http_bindings(&self) -> bool {
        self.services.iter().any(Service::has_http_bindings)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Service {
    pub name: SharedStr,
    pub full_name: SharedStr,
    pub comments: CommentSet,
    pub methods: Vec<Method>,
}

impl Service {
    pub fn has_http_bindings(&self) -> bool {
        self.methods.iter().any(|method| method.http.is_some())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Method {
    pub name: SharedStr,
    pub full_name: SharedStr,
    pub input_type: SharedStr,
    pub output_type: SharedStr,
    pub client_streaming: bool,
    pub server_streaming: bool,
    pub comments: CommentSet,
    pub http: Option<HttpBinding>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Message {
    pub name: SharedStr,
    pub full_name: SharedStr,
    pub comments: CommentSet,
    pub fields: Vec<Field>,
    pub messages: Vec<Message>,
    pub enums: Vec<EnumType>,
    /// Declared oneofs, indexed by [`Field::oneof`]. Includes the synthetic
    /// oneofs protoc creates for proto3 `optional` fields.
    pub oneofs: Vec<Oneof>,
    /// True for the synthetic `...Entry` message protoc creates for a `map`
    /// field.
    pub map_entry: bool,
}

impl Message {
    /// Returns the key and value fields of a map entry message.
    pub fn map_key_value(&self) -> Option<(&Field, &Field)> {
        if !self.map_entry {
            return None;
        }
        let key = self.fields.iter().find(|field| field.number == Some(1))?;
        let value = self.fields.iter().find(|field| field.number == Some(2))?;
        Some((key, value))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Field {
    pub name: SharedStr,
    pub json_name: SharedStr,
    pub number: Option<i32>,
    pub label: Option<FieldLabel>,
    pub kind: FieldKind,
    pub comments: CommentSet,
    /// Index into [`Message::oneofs`] for members of a real oneof. `None` for
    /// proto3 `optional` fields, whose synthetic oneof is not a real choice.
    pub oneof: Option<usize>,
    pub proto3_optional: bool,
    /// Annotated required by `google.api.field_behavior` or Protovalidate.
    pub required: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Oneof {
    pub name: SharedStr,
    /// Annotated `(buf.validate.oneof).required = true`.
    pub required: bool,
    /// True when protoc created this oneof for a proto3 `optional` field.
    pub synthetic: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EnumType {
    pub name: SharedStr,
    pub full_name: SharedStr,
    pub comments: CommentSet,
    pub values: Vec<EnumValue>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EnumValue {
    pub name: SharedStr,
    pub number: i32,
    pub comments: CommentSet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldLabel {
    Optional,
    Required,
    Repeated,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum FieldKind {
    Double,
    Float,
    Int64,
    Uint64,
    Int32,
    Fixed64,
    Fixed32,
    Bool,
    String,
    Group(SharedStr),
    Message(SharedStr),
    Bytes,
    Uint32,
    Enum(SharedStr),
    Sfixed32,
    Sfixed64,
    Sint32,
    Sint64,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpBinding {
    pub verb: HttpVerb,
    pub path: SharedStr,
    pub body: HttpBody,
    pub path_variables: Vec<SharedStr>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpVerb {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl HttpVerb {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HttpBody {
    None,
    Wildcard,
    Field(SharedStr),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommentSet {
    pub leading_detached: Vec<SharedStr>,
    pub leading: Option<SharedStr>,
    pub trailing: Option<SharedStr>,
}

impl CommentSet {
    pub fn is_empty(&self) -> bool {
        self.leading_detached.is_empty() && self.leading.is_none() && self.trailing.is_none()
    }
}

pub fn build_ir(request: &CodeGeneratorRequest) -> CodegenResult<DescriptorIr> {
    let mut messages_by_file = HashMap::new();
    let mut message_index = HashMap::new();

    for file in &request.proto_file {
        let comments = SourceComments::new(file);
        let package = file.package.as_deref().unwrap_or_default();
        let messages = build_messages(
            package,
            &[],
            &file.message_type,
            &comments,
            &[FILE_MESSAGE_TYPE_PATH],
        )?;
        index_messages(&messages, &mut message_index);
        messages_by_file.insert(file_name(file).to_owned(), messages);
    }

    let files = request
        .proto_file
        .iter()
        .map(|file| {
            let name = file_name(file);
            let comments = SourceComments::new(file);
            let package = file.package.as_deref().unwrap_or_default();
            let messages = messages_by_file.remove(name).unwrap_or_default();
            let enums = build_enums(
                package,
                &[],
                &file.enum_type,
                &comments,
                &[FILE_ENUM_TYPE_PATH],
            )?;
            let services = build_services(file, &comments, &message_index)?;

            Ok(ProtoFile {
                name: name.to_owned_opt(),
                package: package.to_owned_opt(),
                messages,
                enums,
                services,
            })
        })
        .collect::<CodegenResult<Vec<_>>>()?;

    Ok(DescriptorIr {
        files,
        descriptor_files: request.proto_file.clone(),
        files_to_generate: request
            .file_to_generate
            .iter()
            .map(|file_name| file_name.as_str().to_owned_opt())
            .collect(),
    })
}

fn build_messages(
    package: &str,
    parents: &[SharedStr],
    descriptors: &[DescriptorProto],
    comments: &SourceComments,
    path_prefix: &[i32],
) -> CodegenResult<Vec<Message>> {
    descriptors
        .iter()
        .enumerate()
        .map(|(message_index, descriptor)| {
            let name = required_name(
                descriptor.name.as_deref(),
                "message",
                format!("message at index {message_index}"),
            )?;
            let path = child_path(path_prefix, message_index)?;
            let full_name = qualify_name(package, parents, name);
            let mut nested_parents = parents.to_vec();
            nested_parents.push(name.to_owned_opt());

            let fields = descriptor
                .field
                .iter()
                .enumerate()
                .map(|(field_index, field)| {
                    let field_path = child_path_with_field(&path, MESSAGE_FIELD_PATH, field_index)?;
                    build_field(field, comments.get(&field_path))
                })
                .collect::<CodegenResult<Vec<_>>>()?;
            let oneofs = descriptor
                .oneof_decl
                .iter()
                .enumerate()
                .map(|(oneof_index, oneof)| {
                    let synthetic = descriptor.field.iter().any(|field| {
                        field.proto3_optional == Some(true)
                            && field
                                .oneof_index
                                .and_then(|index| usize::try_from(index).ok())
                                == Some(oneof_index)
                    });
                    Ok(Oneof {
                        name: oneof.name.as_deref().unwrap_or_default().to_owned_opt(),
                        required: field_options::oneof_is_required(oneof.options.as_option())?,
                        synthetic,
                    })
                })
                .collect::<CodegenResult<Vec<_>>>()?;
            let enums = build_enums(
                package,
                &nested_parents,
                &descriptor.enum_type,
                comments,
                &[path.as_slice(), &[MESSAGE_ENUM_TYPE_PATH]].concat(),
            )?;

            let nested_path_prefix = [path.as_slice(), &[MESSAGE_NESTED_TYPE_PATH]].concat();
            let messages = build_messages(
                package,
                &nested_parents,
                &descriptor.nested_type,
                comments,
                &nested_path_prefix,
            )?;

            Ok(Message {
                name: name.to_owned_opt(),
                full_name,
                comments: comments.get(&path),
                fields,
                messages,
                enums,
                oneofs,
                map_entry: descriptor
                    .options
                    .as_option()
                    .and_then(|options| options.map_entry)
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn build_field(field: &FieldDescriptorProto, comments: CommentSet) -> CodegenResult<Field> {
    let name = required_name(
        field.name.as_deref(),
        "field",
        "field descriptor".to_owned(),
    )?;
    let json_name = field.json_name.as_deref().unwrap_or(name);
    let proto3_optional = field.proto3_optional.unwrap_or(false);
    let oneof = if proto3_optional {
        None
    } else {
        field
            .oneof_index
            .map(|index| {
                usize::try_from(index).kind_context(
                    CodegenErrKind::InvalidDescriptor,
                    format!("field {name} has a negative oneof_index"),
                )
            })
            .transpose()?
    };

    Ok(Field {
        name: name.to_owned_opt(),
        json_name: json_name.to_owned_opt(),
        number: field.number,
        label: field.label.map(FieldLabel::from),
        kind: field_kind(field),
        comments,
        oneof,
        proto3_optional,
        required: field_options::field_is_required(field.options.as_option())?,
    })
}

fn build_enums(
    package: &str,
    parents: &[SharedStr],
    descriptors: &[EnumDescriptorProto],
    comments: &SourceComments,
    path_prefix: &[i32],
) -> CodegenResult<Vec<EnumType>> {
    descriptors
        .iter()
        .enumerate()
        .map(|(enum_index, descriptor)| {
            let name = required_name(
                descriptor.name.as_deref(),
                "enum",
                format!("enum at index {enum_index}"),
            )?;
            let path = child_path(path_prefix, enum_index)?;
            let values = descriptor
                .value
                .iter()
                .enumerate()
                .map(|(value_index, value)| {
                    let value_name = required_name(
                        value.name.as_deref(),
                        "enum value",
                        format!("enum {name} value at index {value_index}"),
                    )?;
                    let value_path = child_path_with_field(&path, ENUM_VALUE_PATH, value_index)?;
                    Ok(EnumValue {
                        name: value_name.to_owned_opt(),
                        number: value.number.unwrap_or_default(),
                        comments: comments.get(&value_path),
                    })
                })
                .collect::<CodegenResult<Vec<_>>>()?;

            Ok(EnumType {
                name: name.to_owned_opt(),
                full_name: qualify_name(package, parents, name),
                comments: comments.get(&path),
                values,
            })
        })
        .collect()
}

fn build_services(
    file: &FileDescriptorProto,
    comments: &SourceComments,
    message_index: &HashMap<SharedStr, Message>,
) -> CodegenResult<Vec<Service>> {
    let package = file.package.as_deref().unwrap_or_default();
    file.service
        .iter()
        .enumerate()
        .map(|(service_index, service)| {
            let name = required_name(
                service.name.as_deref(),
                "service",
                format!("service at index {service_index}"),
            )?;
            let path = vec![FILE_SERVICE_PATH, index_to_path(service_index)?];
            let full_name = qualify_name(package, &[], name);
            let methods = build_methods(service, &full_name, &path, comments, message_index)?;

            Ok(Service {
                name: name.to_owned_opt(),
                full_name,
                comments: comments.get(&path),
                methods,
            })
        })
        .collect()
}

fn build_methods(
    service: &ServiceDescriptorProto,
    service_full_name: &SharedStr,
    service_path: &[i32],
    comments: &SourceComments,
    message_index: &HashMap<SharedStr, Message>,
) -> CodegenResult<Vec<Method>> {
    service
        .method
        .iter()
        .enumerate()
        .map(|(method_index, method)| {
            let name = required_name(
                method.name.as_deref(),
                "method",
                format!("{service_full_name} method at index {method_index}"),
            )?;
            let method_path =
                child_path_with_field(service_path, SERVICE_METHOD_PATH, method_index)?;
            let input_type = normalize_type_name(method.input_type.as_deref().unwrap_or_default());
            let output_type =
                normalize_type_name(method.output_type.as_deref().unwrap_or_default());
            let http = http::extract_http_binding(method)?;

            if let Some(binding) = &http {
                validate_http_binding(method, binding, message_index)?;
            }

            Ok(Method {
                name: name.to_owned_opt(),
                full_name: format!("{service_full_name}.{name}").to_owned_opt(),
                input_type,
                output_type,
                client_streaming: method.client_streaming.unwrap_or(false),
                server_streaming: method.server_streaming.unwrap_or(false),
                comments: comments.get(&method_path),
                http,
            })
        })
        .collect()
}

fn validate_http_binding(
    method: &MethodDescriptorProto,
    binding: &HttpBinding,
    message_index: &HashMap<SharedStr, Message>,
) -> CodegenResult<()> {
    let input_type = normalize_type_name(method.input_type.as_deref().ok_or_else(|| {
        UniError::from_kind_context(
            CodegenErrKind::InvalidDescriptor,
            format!(
                "method {:?} has an HTTP binding but no input_type",
                method.name.as_deref().unwrap_or("<unknown>")
            ),
        )
    })?);
    if input_type.as_ref() == "google.protobuf.Empty" && binding.path_variables.is_empty() {
        return Ok(());
    }

    let input = message_index.get(&input_type).ok_or_else(|| {
        UniError::from_kind_context(
            CodegenErrKind::InvalidDescriptor,
            format!(
                "method {:?} input message {input_type:?} was not found",
                method.name.as_deref().unwrap_or("<unknown>")
            ),
        )
    })?;

    for path_variable in &binding.path_variables {
        resolve_field_path(input, path_variable.as_ref(), |name| {
            message_index.get(name)
        })?;
    }

    Ok(())
}

/// Resolves a dotted HTTP field path (`a.b.c`) to the fields along it. Every
/// field except the last must be a singular message.
pub fn resolve_field_path<'a>(
    message: &'a Message,
    path: &str,
    lookup: impl Fn(&str) -> Option<&'a Message>,
) -> CodegenResult<Vec<&'a Field>> {
    let not_found = || {
        UniError::from_kind_context(
            CodegenErrKind::PathFieldNotFound,
            format!(
                "path field not found: {path} on request message {}",
                message.full_name.as_ref()
            ),
        )
    };
    let mut current = message;
    let mut fields: Vec<&Field> = Vec::new();
    for segment in path.split('.') {
        if let Some(parent) = fields.last() {
            current = match &parent.kind {
                FieldKind::Message(name) | FieldKind::Group(name)
                    if parent.label != Some(FieldLabel::Repeated) =>
                {
                    lookup(name.as_ref()).ok_or_else(not_found)?
                }
                _ => {
                    return Err(UniError::from_kind_context(
                        CodegenErrKind::UnsupportedHttpRule,
                        format!("path field {path} must only traverse singular message fields"),
                    ));
                }
            };
        }
        let field = current
            .fields
            .iter()
            .find(|field| field.name.as_ref() == segment)
            .ok_or_else(not_found)?;
        fields.push(field);
    }
    Ok(fields)
}

fn field_kind(field: &FieldDescriptorProto) -> FieldKind {
    use descriptor::field_descriptor_proto::Type;

    match field.r#type {
        Some(Type::TYPE_DOUBLE) => FieldKind::Double,
        Some(Type::TYPE_FLOAT) => FieldKind::Float,
        Some(Type::TYPE_INT64) => FieldKind::Int64,
        Some(Type::TYPE_UINT64) => FieldKind::Uint64,
        Some(Type::TYPE_INT32) => FieldKind::Int32,
        Some(Type::TYPE_FIXED64) => FieldKind::Fixed64,
        Some(Type::TYPE_FIXED32) => FieldKind::Fixed32,
        Some(Type::TYPE_BOOL) => FieldKind::Bool,
        Some(Type::TYPE_STRING) => FieldKind::String,
        Some(Type::TYPE_GROUP) => FieldKind::Group(normalize_type_name(
            field.type_name.as_deref().unwrap_or_default(),
        )),
        Some(Type::TYPE_MESSAGE) => FieldKind::Message(normalize_type_name(
            field.type_name.as_deref().unwrap_or_default(),
        )),
        Some(Type::TYPE_BYTES) => FieldKind::Bytes,
        Some(Type::TYPE_UINT32) => FieldKind::Uint32,
        Some(Type::TYPE_ENUM) => FieldKind::Enum(normalize_type_name(
            field.type_name.as_deref().unwrap_or_default(),
        )),
        Some(Type::TYPE_SFIXED32) => FieldKind::Sfixed32,
        Some(Type::TYPE_SFIXED64) => FieldKind::Sfixed64,
        Some(Type::TYPE_SINT32) => FieldKind::Sint32,
        Some(Type::TYPE_SINT64) => FieldKind::Sint64,
        None => FieldKind::Unknown,
    }
}

fn index_messages(messages: &[Message], message_index: &mut HashMap<SharedStr, Message>) {
    for message in messages {
        message_index.insert(message.full_name.clone(), message.clone());
        index_messages(&message.messages, message_index);
    }
}

fn find_nested_enum<'a>(message: &'a Message, full_name: &str) -> Option<&'a EnumType> {
    message
        .enums
        .iter()
        .find(|enum_type| enum_type.full_name.as_ref() == full_name)
        .or_else(|| {
            message
                .messages
                .iter()
                .find_map(|message| find_nested_enum(message, full_name))
        })
}

fn find_message<'a>(message: &'a Message, full_name: &str) -> Option<&'a Message> {
    if message.full_name.as_ref() == full_name {
        return Some(message);
    }

    message
        .messages
        .iter()
        .find_map(|message| find_message(message, full_name))
}

fn required_name<'a>(
    value: Option<&'a str>,
    kind: &'static str,
    context: String,
) -> CodegenResult<&'a str> {
    value.filter(|name| !name.is_empty()).ok_or_else(|| {
        UniError::from_kind_context(
            CodegenErrKind::InvalidDescriptor,
            format!("{kind} name is missing in {context}"),
        )
    })
}

fn qualify_name(package: &str, parents: &[SharedStr], name: &str) -> SharedStr {
    let mut full_name = String::new();
    if !package.is_empty() {
        full_name.push_str(package);
    }
    for parent in parents {
        if !full_name.is_empty() {
            full_name.push('.');
        }
        full_name.push_str(parent.as_ref());
    }
    if !full_name.is_empty() {
        full_name.push('.');
    }
    full_name.push_str(name);
    full_name.to_owned_opt()
}

fn normalize_type_name(value: &str) -> SharedStr {
    value.strip_prefix('.').unwrap_or(value).to_owned_opt()
}

fn file_name(file: &FileDescriptorProto) -> &str {
    file.name.as_deref().unwrap_or_default()
}

fn child_path(path_prefix: &[i32], index: usize) -> CodegenResult<Vec<i32>> {
    let mut path = path_prefix.to_vec();
    path.push(index_to_path(index)?);
    Ok(path)
}

fn child_path_with_field(
    path_prefix: &[i32],
    field_number: i32,
    index: usize,
) -> CodegenResult<Vec<i32>> {
    let mut path = path_prefix.to_vec();
    path.push(field_number);
    path.push(index_to_path(index)?);
    Ok(path)
}

fn index_to_path(index: usize) -> CodegenResult<i32> {
    i32::try_from(index).kind_context(
        CodegenErrKind::InvalidDescriptor,
        "descriptor index does not fit in source-code-info path",
    )
}

impl From<descriptor::field_descriptor_proto::Label> for FieldLabel {
    fn from(value: descriptor::field_descriptor_proto::Label) -> Self {
        match value {
            descriptor::field_descriptor_proto::Label::LABEL_OPTIONAL => Self::Optional,
            descriptor::field_descriptor_proto::Label::LABEL_REQUIRED => Self::Required,
            descriptor::field_descriptor_proto::Label::LABEL_REPEATED => Self::Repeated,
        }
    }
}

struct SourceComments {
    by_path: HashMap<Vec<i32>, CommentSet>,
}

impl SourceComments {
    fn new(file: &FileDescriptorProto) -> Self {
        let by_path = file
            .source_code_info
            .as_option()
            .into_iter()
            .flat_map(|source_code_info| source_code_info.location.iter())
            .map(|location| (location.path.clone(), comment_set(location)))
            .filter(|(_, comments)| !comments.is_empty())
            .collect();

        Self { by_path }
    }

    fn get(&self, path: &[i32]) -> CommentSet {
        self.by_path.get(path).cloned().unwrap_or_default()
    }
}

fn comment_set(location: &descriptor::source_code_info::Location) -> CommentSet {
    CommentSet {
        leading_detached: location
            .leading_detached_comments
            .iter()
            .filter(|comment| !comment.is_empty())
            .map(|comment| comment.as_str().to_owned_opt())
            .collect(),
        leading: non_empty_comment(location.leading_comments.as_deref()),
        trailing: non_empty_comment(location.trailing_comments.as_deref()),
    }
}

fn non_empty_comment(comment: Option<&str>) -> Option<SharedStr> {
    comment
        .filter(|comment| !comment.is_empty())
        .map(|comment| comment.to_owned_opt())
}

#[cfg(test)]
mod tests {
    use buffa::encoding::{Tag, WireType};
    use buffa::{MessageField, UnknownField, UnknownFieldData};
    use connectrpc_codegen::codegen::descriptor::{
        DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
        FileDescriptorProto, MessageOptions, MethodDescriptorProto, MethodOptions,
        OneofDescriptorProto, ServiceDescriptorProto, SourceCodeInfo,
        field_descriptor_proto::{Label, Type},
        source_code_info::Location,
    };

    use super::{FieldKind, HttpBody, HttpVerb, build_ir};
    use crate::CodeGeneratorRequest;
    use crate::internal::field_options::test_support::{
        field_behavior_required, protovalidate_field_required, protovalidate_oneof_required,
    };
    use crate::internal::http::HTTP_EXTENSION_NUMBER;

    #[test]
    fn extracts_top_level_and_nested_enums_with_comments() {
        let mut message = request_message(vec![enum_field("kind", 1, ".test.v1.TestRequest.Kind")]);
        message.enum_type = vec![enum_descriptor("Kind", &["KIND_UNSPECIFIED", "KIND_A"])];
        let mut file = file_descriptor(
            vec![message],
            Vec::new(),
            Some(SourceCodeInfo {
                location: vec![
                    location(vec![5, 0], "Color docs\n"),
                    location(vec![5, 0, 2, 1], "Red docs\n"),
                    location(vec![4, 0, 4, 0], "Kind docs\n"),
                ],
                ..Default::default()
            }),
        );
        file.enum_type = vec![enum_descriptor("Color", &["COLOR_UNSPECIFIED", "RED"])];

        let ir = build_ir(&CodeGeneratorRequest {
            proto_file: vec![file],
            ..Default::default()
        })
        .unwrap();

        let color = ir.enum_type("test.v1.Color").expect("top-level enum");
        assert_eq!(
            color
                .values
                .iter()
                .map(|value| value.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["COLOR_UNSPECIFIED", "RED"]
        );
        assert_eq!(color.values[1].number, 1);
        assert_eq!(leading(color), Some("Color docs\n"));
        assert_eq!(
            color.values[1].comments.leading.as_deref(),
            Some("Red docs\n")
        );

        let kind = ir
            .enum_type("test.v1.TestRequest.Kind")
            .expect("nested enum");
        assert_eq!(kind.values.len(), 2);
        assert_eq!(leading(kind), Some("Kind docs\n"));
        assert_eq!(
            ir.files[0].messages[0].fields[0].kind,
            FieldKind::Enum("test.v1.TestRequest.Kind".into())
        );
    }

    #[test]
    fn marks_map_entries_and_exposes_key_value() {
        let mut entry = DescriptorProto {
            name: Some("LabelsEntry".into()),
            field: vec![string_field("key", 1), string_field("value", 2)],
            ..Default::default()
        };
        entry.options = MessageField::some(MessageOptions {
            map_entry: Some(true),
            ..Default::default()
        });
        let mut labels = message_field("labels", 1, ".test.v1.TestRequest.LabelsEntry");
        labels.label = Some(Label::LABEL_REPEATED);
        let mut message = request_message(vec![labels]);
        message.nested_type = vec![entry];

        let ir = build_ir(&CodeGeneratorRequest {
            proto_file: vec![file_descriptor(vec![message], Vec::new(), None)],
            ..Default::default()
        })
        .unwrap();

        let request = &ir.files[0].messages[0];
        assert!(!request.map_entry);
        let entry = ir.message("test.v1.TestRequest.LabelsEntry").unwrap();
        assert!(entry.map_entry);
        let (key, value) = entry.map_key_value().unwrap();
        assert_eq!((key.name.as_ref(), value.name.as_ref()), ("key", "value"));
    }

    #[test]
    fn records_real_and_synthetic_oneofs() {
        let mut first = string_field("first", 1);
        first.oneof_index = Some(0);
        let mut second = string_field("second", 2);
        second.oneof_index = Some(0);
        let mut optional = string_field("maybe", 3);
        optional.oneof_index = Some(1);
        optional.proto3_optional = Some(true);
        let mut message = request_message(vec![first, second, optional]);
        message.oneof_decl = vec![
            OneofDescriptorProto {
                name: Some("choice".into()),
                options: MessageField::some(protovalidate_oneof_required()),
                ..Default::default()
            },
            OneofDescriptorProto {
                name: Some("_maybe".into()),
                ..Default::default()
            },
        ];

        let ir = build_ir(&CodeGeneratorRequest {
            proto_file: vec![file_descriptor(vec![message], Vec::new(), None)],
            ..Default::default()
        })
        .unwrap();

        let message = &ir.files[0].messages[0];
        assert_eq!(message.oneofs.len(), 2);
        assert_eq!(message.oneofs[0].name.as_ref(), "choice");
        assert!(message.oneofs[0].required);
        assert!(!message.oneofs[0].synthetic);
        assert!(message.oneofs[1].synthetic);
        assert_eq!(message.fields[0].oneof, Some(0));
        assert_eq!(message.fields[1].oneof, Some(0));
        assert_eq!(message.fields[2].oneof, None);
        assert!(message.fields[2].proto3_optional);
    }

    #[test]
    fn marks_annotated_fields_required() {
        let plain = string_field("plain", 1);
        let mut unpacked = string_field("unpacked", 2);
        unpacked.options = MessageField::some(field_behavior_required(false));
        let mut packed = string_field("packed", 3);
        packed.options = MessageField::some(field_behavior_required(true));
        let mut validated = string_field("validated", 4);
        validated.options = MessageField::some(protovalidate_field_required());

        let ir = build_ir(&CodeGeneratorRequest {
            proto_file: vec![file_descriptor(
                vec![request_message(vec![plain, unpacked, packed, validated])],
                Vec::new(),
                None,
            )],
            ..Default::default()
        })
        .unwrap();

        let required = ir.files[0].messages[0]
            .fields
            .iter()
            .map(|field| (field.name.as_ref(), field.required))
            .collect::<Vec<_>>();
        assert_eq!(
            required,
            vec![
                ("plain", false),
                ("unpacked", true),
                ("packed", true),
                ("validated", true)
            ]
        );
    }

    fn enum_descriptor(name: &str, values: &[&str]) -> EnumDescriptorProto {
        EnumDescriptorProto {
            name: Some(name.into()),
            value: values
                .iter()
                .enumerate()
                .map(|(number, value)| EnumValueDescriptorProto {
                    name: Some((*value).into()),
                    number: Some(number as i32),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn extracts_unary_post_with_path_fields_and_single_field_body() {
        let request = CodeGeneratorRequest {
            proto_file: vec![file_descriptor(
                vec![request_message(vec![
                    string_field("data", 1),
                    enum_field("test_type", 2, ".test.v1.Tester"),
                    message_field("tester", 8, ".test.v1.Nested"),
                ])],
                vec![service_descriptor(vec![method_descriptor(
                    "DoTest",
                    ".test.v1.TestRequest",
                    Some(http_rule(
                        4,
                        "/test/{data}/testing/{test_type}",
                        Some("tester"),
                    )),
                )])],
                None,
            )],
            ..Default::default()
        };

        let ir = build_ir(&request).unwrap();
        let method = &ir.files[0].services[0].methods[0];
        let binding = method.http.as_ref().expect("method has HTTP binding");

        assert_eq!(binding.verb, HttpVerb::Post);
        assert_eq!(binding.path.as_ref(), "/test/{data}/testing/{test_type}");
        assert_eq!(binding.body, HttpBody::Field("tester".into()));
        assert_eq!(
            binding
                .path_variables
                .iter()
                .map(|field| field.as_ref())
                .collect::<Vec<_>>(),
            vec!["data", "test_type"]
        );
    }

    #[test]
    fn missing_path_field_is_an_error() {
        let request = CodeGeneratorRequest {
            proto_file: vec![file_descriptor(
                vec![request_message(vec![string_field("data", 1)])],
                vec![service_descriptor(vec![method_descriptor(
                    "DoTest",
                    ".test.v1.TestRequest",
                    Some(http_rule(2, "/test/{missing}", None)),
                )])],
                None,
            )],
            ..Default::default()
        };

        let err = build_ir(&request).unwrap_err();

        assert!(err.to_string().contains("path field not found: missing"));
    }

    #[test]
    fn unsupported_path_template_is_an_error() {
        let request = CodeGeneratorRequest {
            proto_file: vec![file_descriptor(
                vec![request_message(vec![string_field("name", 1)])],
                vec![service_descriptor(vec![method_descriptor(
                    "DoTest",
                    ".test.v1.TestRequest",
                    Some(http_rule(2, "/v1/{name=messages/*}", None)),
                )])],
                None,
            )],
            ..Default::default()
        };

        let err = build_ir(&request).unwrap_err();

        assert!(
            err.to_string()
                .contains("unsupported google.api.http path template")
        );
    }

    #[test]
    fn source_comments_are_attached_to_ir_nodes() {
        let request = CodeGeneratorRequest {
            proto_file: vec![file_descriptor(
                vec![request_message(vec![string_field("data", 1)])],
                vec![service_descriptor(vec![method_descriptor(
                    "DoTest",
                    ".test.v1.TestRequest",
                    Some(http_rule(2, "/test/{data}", None)),
                )])],
                Some(SourceCodeInfo {
                    location: vec![
                        location(vec![4, 0], "Request docs\n"),
                        location(vec![4, 0, 2, 0], "Data docs\n"),
                        location(vec![6, 0], "Service docs\n"),
                        location(vec![6, 0, 2, 0], "Method docs\n"),
                    ],
                    ..Default::default()
                }),
            )],
            ..Default::default()
        };

        let ir = build_ir(&request).unwrap();
        let file = &ir.files[0];
        let message = &file.messages[0];
        let field = &message.fields[0];
        let service = &file.services[0];
        let method = &service.methods[0];

        assert_eq!(leading(message), Some("Request docs\n"));
        assert_eq!(leading(field), Some("Data docs\n"));
        assert_eq!(leading(service), Some("Service docs\n"));
        assert_eq!(leading(method), Some("Method docs\n"));
    }

    fn leading<T>(node: &T) -> Option<&str>
    where
        T: HasComments,
    {
        node.comments()
            .leading
            .as_ref()
            .map(|comment| comment.as_ref())
    }

    trait HasComments {
        fn comments(&self) -> &super::CommentSet;
    }

    impl HasComments for super::Message {
        fn comments(&self) -> &super::CommentSet {
            &self.comments
        }
    }

    impl HasComments for super::Field {
        fn comments(&self) -> &super::CommentSet {
            &self.comments
        }
    }

    impl HasComments for super::Service {
        fn comments(&self) -> &super::CommentSet {
            &self.comments
        }
    }

    impl HasComments for super::EnumType {
        fn comments(&self) -> &super::CommentSet {
            &self.comments
        }
    }

    impl HasComments for super::Method {
        fn comments(&self) -> &super::CommentSet {
            &self.comments
        }
    }

    fn file_descriptor(
        messages: Vec<DescriptorProto>,
        services: Vec<ServiceDescriptorProto>,
        source_code_info: Option<SourceCodeInfo>,
    ) -> FileDescriptorProto {
        FileDescriptorProto {
            name: Some("test/v1/test.proto".into()),
            package: Some("test.v1".into()),
            message_type: messages,
            service: services,
            source_code_info: source_code_info.map_or_else(MessageField::none, MessageField::some),
            ..Default::default()
        }
    }

    fn request_message(fields: Vec<FieldDescriptorProto>) -> DescriptorProto {
        DescriptorProto {
            name: Some("TestRequest".into()),
            field: fields,
            ..Default::default()
        }
    }

    fn service_descriptor(methods: Vec<MethodDescriptorProto>) -> ServiceDescriptorProto {
        ServiceDescriptorProto {
            name: Some("TestService".into()),
            method: methods,
            ..Default::default()
        }
    }

    fn method_descriptor(
        name: &str,
        input_type: &str,
        http_rule: Option<Vec<u8>>,
    ) -> MethodDescriptorProto {
        MethodDescriptorProto {
            name: Some(name.into()),
            input_type: Some(input_type.into()),
            output_type: Some(".test.v1.TestReply".into()),
            options: http_rule.map_or_else(MessageField::none, method_options),
            ..Default::default()
        }
    }

    fn string_field(name: &str, number: i32) -> FieldDescriptorProto {
        typed_field(name, number, Type::TYPE_STRING, None)
    }

    fn enum_field(name: &str, number: i32, type_name: &str) -> FieldDescriptorProto {
        typed_field(name, number, Type::TYPE_ENUM, Some(type_name))
    }

    fn message_field(name: &str, number: i32, type_name: &str) -> FieldDescriptorProto {
        typed_field(name, number, Type::TYPE_MESSAGE, Some(type_name))
    }

    fn typed_field(
        name: &str,
        number: i32,
        r#type: Type,
        type_name: Option<&str>,
    ) -> FieldDescriptorProto {
        FieldDescriptorProto {
            name: Some(name.into()),
            number: Some(number),
            label: Some(Label::LABEL_OPTIONAL),
            r#type: Some(r#type),
            type_name: type_name.map(Into::into),
            json_name: Some(name.into()),
            ..Default::default()
        }
    }

    fn method_options(
        http_rule: Vec<u8>,
    ) -> MessageField<MethodOptions, buffa::Inline<MethodOptions>> {
        let mut options = MethodOptions::default();
        options.__buffa_unknown_fields.push(UnknownField {
            number: HTTP_EXTENSION_NUMBER,
            data: UnknownFieldData::LengthDelimited(http_rule),
        });
        MessageField::some(options)
    }

    fn http_rule(verb_field: u32, path: &str, body: Option<&str>) -> Vec<u8> {
        let mut bytes = Vec::new();
        Tag::new(verb_field, WireType::LengthDelimited).encode(&mut bytes);
        buffa::types::encode_string(path, &mut bytes);
        if let Some(body) = body {
            Tag::new(7, WireType::LengthDelimited).encode(&mut bytes);
            buffa::types::encode_string(body, &mut bytes);
        }
        bytes
    }

    fn location(path: Vec<i32>, leading_comments: &str) -> Location {
        Location {
            path,
            leading_comments: Some(leading_comments.into()),
            ..Default::default()
        }
    }
}
