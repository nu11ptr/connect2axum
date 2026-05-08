use std::collections::HashMap;

use flexstr::{SharedStr, ToOwnedFlexStr as _};
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};
use crate::ir::{
    CommentSet, DescriptorIr, Field, FieldKind, HttpBinding, HttpBody, Message, Method, ProtoFile,
};
use crate::options::CodegenOptions;
use crate::resolver::{RustPath, TypeResolver};

const GOOGLE_EMPTY: &str = "google.protobuf.Empty";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileShapes {
    pub request_shapes: Vec<RequestShape>,
    pub generated_dtos: Vec<GeneratedDto>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestShape {
    pub method: SharedStr,
    pub request_type: RustPath,
    pub request_view_type: RustPath,
    pub path_fields: Vec<ShapeField>,
    pub query_shape: Option<RequestPartShape>,
    pub body_shape: Option<RequestPartShape>,
    pub reconstruction: RequestReconstruction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShapeField {
    pub field: Field,
    pub rust_type: RustPath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestPartShape {
    VerbatimRequest {
        rust_type: RustPath,
    },
    ExistingMessage {
        field: ShapeField,
        rust_type: RustPath,
    },
    GeneratedDto {
        name: SharedStr,
        rust_type: RustPath,
        fields: Vec<ShapeField>,
    },
}

impl RequestPartShape {
    pub fn description(&self) -> String {
        match self {
            Self::VerbatimRequest { rust_type } => format!("verbatim {}", rust_type.as_str()),
            Self::ExistingMessage { field, rust_type } => {
                format!("{} as {}", field.field.name.as_ref(), rust_type.as_str())
            }
            Self::GeneratedDto {
                name,
                rust_type,
                fields,
            } => {
                let fields = fields
                    .iter()
                    .map(|field| field.field.name.as_ref())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name} ({}) [{fields}]", rust_type.as_str())
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestReconstruction {
    Empty,
    VerbatimBody,
    VerbatimQuery,
    FromParts { fields: Vec<FieldAssignment> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldAssignment {
    pub field: SharedStr,
    pub source: FieldSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldSource {
    Path,
    Body,
    Query,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedDto {
    pub kind: GeneratedDtoKind,
    pub name: SharedStr,
    pub rust_type: RustPath,
    pub source_message: SharedStr,
    pub comments: CommentSet,
    pub fields: Vec<ShapeField>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GeneratedDtoKind {
    Body,
    Query,
}

impl GeneratedDtoKind {
    fn suffix(self, options: &CodegenOptions) -> &str {
        match self {
            Self::Body => options.body_message_suffix.as_ref(),
            Self::Query => options.query_message_suffix.as_ref(),
        }
    }
}

pub fn plan_file_shapes(
    ir: &DescriptorIr,
    proto_file: &ProtoFile,
    options: &CodegenOptions,
) -> CodegenResult<FileShapes> {
    let resolver = TypeResolver::new(ir, options);
    let mut planner = RequestPlanner::new(ir, &resolver, options);
    let mut request_shapes = Vec::new();

    for service in &proto_file.services {
        for method in &service.methods {
            if method.http.is_some() {
                request_shapes.push(planner.plan_method(method)?);
            }
        }
    }

    Ok(FileShapes {
        request_shapes,
        generated_dtos: planner.generated_dtos,
    })
}

struct RequestPlanner<'a> {
    ir: &'a DescriptorIr,
    resolver: &'a TypeResolver<'a>,
    options: &'a CodegenOptions,
    generated_dtos: Vec<GeneratedDto>,
    dto_registry: HashMap<(GeneratedDtoKind, SharedStr), Vec<GeneratedDto>>,
}

impl<'a> RequestPlanner<'a> {
    fn new(
        ir: &'a DescriptorIr,
        resolver: &'a TypeResolver<'a>,
        options: &'a CodegenOptions,
    ) -> Self {
        Self {
            ir,
            resolver,
            options,
            generated_dtos: Vec::new(),
            dto_registry: HashMap::new(),
        }
    }

    fn plan_method(&mut self, method: &Method) -> CodegenResult<RequestShape> {
        let binding = method.http.as_ref().ok_or_else(|| {
            UniError::from_kind_context(
                CodegenErrKind::InvalidHttpAnnotation,
                format!("method {} has no HTTP binding", method.full_name.as_ref()),
            )
        })?;
        let request_message = self.request_message(method)?;
        let request_type = self
            .resolver
            .owned_message_type(method.input_type.as_ref())?;
        let request_view_type = self
            .resolver
            .view_message_type(method.input_type.as_ref())?;
        let mut work = RequestWork::new(request_message.clone());

        let path_fields = self.take_path_fields(&mut work, binding)?;
        let body_shape = self.plan_body(&mut work, binding)?;
        let query_shape = self.plan_query(&mut work)?;
        let reconstruction = reconstruction_for(
            &request_message,
            binding,
            &path_fields,
            &body_shape,
            &query_shape,
        );
        validate_streaming_shape(method, binding, &path_fields, &query_shape, &body_shape)?;

        Ok(RequestShape {
            method: method.full_name.clone(),
            request_type,
            request_view_type,
            path_fields,
            query_shape,
            body_shape,
            reconstruction,
        })
    }

    fn request_message(&self, method: &Method) -> CodegenResult<Message> {
        if method.input_type.as_ref() == GOOGLE_EMPTY {
            return Ok(Message {
                name: "Empty".into(),
                full_name: GOOGLE_EMPTY.into(),
                comments: CommentSet::default(),
                fields: Vec::new(),
                messages: Vec::new(),
            });
        }

        self.ir
            .message(method.input_type.as_ref())
            .cloned()
            .ok_or_else(|| {
                UniError::from_kind_context(
                    CodegenErrKind::RequestMessageNotFound,
                    format!(
                        "request message {} for {} was not found",
                        method.input_type.as_ref(),
                        method.full_name.as_ref()
                    ),
                )
            })
    }

    fn take_path_fields(
        &self,
        work: &mut RequestWork,
        binding: &HttpBinding,
    ) -> CodegenResult<Vec<ShapeField>> {
        binding
            .path_variables
            .iter()
            .map(|path_variable| {
                let field = work.remove_field(path_variable.as_ref()).ok_or_else(|| {
                    UniError::from_kind_context(
                        CodegenErrKind::PathFieldNotFound,
                        format!(
                            "path field not found: {} on request message {}",
                            path_variable.as_ref(),
                            work.message.full_name.as_ref()
                        ),
                    )
                })?;
                self.shape_field(field)
            })
            .collect()
    }

    fn plan_body(
        &mut self,
        work: &mut RequestWork,
        binding: &HttpBinding,
    ) -> CodegenResult<Option<RequestPartShape>> {
        match &binding.body {
            HttpBody::None => Ok(None),
            HttpBody::Wildcard => {
                if work.is_intact() {
                    work.remove_all_fields();
                    Ok(Some(RequestPartShape::VerbatimRequest {
                        rust_type: self
                            .resolver
                            .owned_message_type(work.message.full_name.as_ref())?,
                    }))
                } else {
                    let fields = self.shape_fields(work.remove_all_fields())?;
                    Ok(Some(self.generated_dto_part(
                        GeneratedDtoKind::Body,
                        &work.message,
                        fields,
                    )))
                }
            }
            HttpBody::Field(body_field) => {
                let intact_single_field = work.intact_single_field();
                let field = work.remove_field(body_field.as_ref()).ok_or_else(|| {
                    UniError::from_kind_context(
                        CodegenErrKind::BodyFieldNotFound,
                        format!(
                            "body field not found: {} on request message {}",
                            body_field.as_ref(),
                            work.message.full_name.as_ref()
                        ),
                    )
                })?;

                if let FieldKind::Message(type_name) = field.kind.clone() {
                    let shape_field = self.shape_field(field)?;
                    return Ok(Some(RequestPartShape::ExistingMessage {
                        rust_type: self.resolver.owned_message_type(type_name.as_ref())?,
                        field: shape_field,
                    }));
                }

                if intact_single_field {
                    Ok(Some(RequestPartShape::VerbatimRequest {
                        rust_type: self
                            .resolver
                            .owned_message_type(work.message.full_name.as_ref())?,
                    }))
                } else {
                    let fields = vec![self.shape_field(field)?];
                    Ok(Some(self.generated_dto_part(
                        GeneratedDtoKind::Body,
                        &work.message,
                        fields,
                    )))
                }
            }
        }
    }

    fn plan_query(&mut self, work: &mut RequestWork) -> CodegenResult<Option<RequestPartShape>> {
        if work.is_empty() {
            Ok(None)
        } else if work.is_intact() {
            Ok(Some(RequestPartShape::VerbatimRequest {
                rust_type: self
                    .resolver
                    .owned_message_type(work.message.full_name.as_ref())?,
            }))
        } else {
            let fields = self.shape_fields(work.remove_all_fields())?;
            Ok(Some(self.generated_dto_part(
                GeneratedDtoKind::Query,
                &work.message,
                fields,
            )))
        }
    }

    fn generated_dto_part(
        &mut self,
        kind: GeneratedDtoKind,
        source_message: &Message,
        fields: Vec<ShapeField>,
    ) -> RequestPartShape {
        let dto = self.get_or_create_dto(kind, source_message, fields);
        RequestPartShape::GeneratedDto {
            name: dto.name.clone(),
            rust_type: dto.rust_type.clone(),
            fields: dto.fields.clone(),
        }
    }

    fn get_or_create_dto(
        &mut self,
        kind: GeneratedDtoKind,
        source_message: &Message,
        fields: Vec<ShapeField>,
    ) -> GeneratedDto {
        let registry_key = (kind, source_message.full_name.clone());
        let messages = self.dto_registry.entry(registry_key).or_default();

        if let Some(existing) = messages
            .iter()
            .find(|dto| same_shape_fields(&dto.fields, &fields))
        {
            return existing.clone();
        }

        let name = generated_dto_name(kind, source_message, messages.len(), self.options);
        let dto = GeneratedDto {
            kind,
            rust_type: RustPath::new(name.as_ref()),
            name,
            source_message: source_message.full_name.clone(),
            comments: source_message.comments.clone(),
            fields,
        };
        messages.push(dto.clone());
        self.generated_dtos.push(dto.clone());
        dto
    }

    fn shape_field(&self, field: Field) -> CodegenResult<ShapeField> {
        let rust_type = self.resolver.field_rust_type(&field)?;
        Ok(ShapeField { field, rust_type })
    }

    fn shape_fields(&self, fields: Vec<Field>) -> CodegenResult<Vec<ShapeField>> {
        fields
            .into_iter()
            .map(|field| self.shape_field(field))
            .collect()
    }
}

fn validate_streaming_shape(
    method: &Method,
    binding: &HttpBinding,
    path_fields: &[ShapeField],
    query_shape: &Option<RequestPartShape>,
    body_shape: &Option<RequestPartShape>,
) -> CodegenResult<()> {
    if !method.client_streaming {
        return Ok(());
    }

    if !path_fields.is_empty() {
        return Err(UniError::from_kind_context(
            CodegenErrKind::UnsupportedHttpRule,
            format!(
                "client or bidi streaming REST method {} cannot bind path parameters",
                method.full_name.as_ref()
            ),
        ));
    }

    if query_shape.is_some() {
        return Err(UniError::from_kind_context(
            CodegenErrKind::UnsupportedHttpRule,
            format!(
                "client or bidi streaming REST method {} cannot bind query parameters",
                method.full_name.as_ref()
            ),
        ));
    }

    if !matches!(binding.body, HttpBody::Wildcard) {
        return Err(UniError::from_kind_context(
            CodegenErrKind::UnsupportedHttpRule,
            format!(
                "client or bidi streaming REST method {} must use body: \"*\"",
                method.full_name.as_ref()
            ),
        ));
    }

    if !matches!(body_shape, Some(RequestPartShape::VerbatimRequest { .. })) {
        return Err(UniError::from_kind_context(
            CodegenErrKind::UnsupportedHttpRule,
            format!(
                "client or bidi streaming REST method {} must stream complete request messages",
                method.full_name.as_ref()
            ),
        ));
    }

    Ok(())
}

#[derive(Clone)]
struct RequestWork {
    message: Message,
    fields: Vec<Field>,
    original_field_count: usize,
}

impl RequestWork {
    fn new(message: Message) -> Self {
        let original_field_count = message.fields.len();
        let fields = message.fields.clone();
        Self {
            message,
            fields,
            original_field_count,
        }
    }

    fn remove_field(&mut self, name: &str) -> Option<Field> {
        self.fields
            .iter()
            .position(|field| field.name.as_ref() == name)
            .map(|index| self.fields.remove(index))
    }

    fn remove_all_fields(&mut self) -> Vec<Field> {
        std::mem::take(&mut self.fields)
    }

    fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    fn is_intact(&self) -> bool {
        self.fields.len() == self.original_field_count
    }

    fn intact_single_field(&self) -> bool {
        self.is_intact() && self.original_field_count == 1
    }
}

fn reconstruction_for(
    request_message: &Message,
    binding: &HttpBinding,
    path_fields: &[ShapeField],
    body_shape: &Option<RequestPartShape>,
    query_shape: &Option<RequestPartShape>,
) -> RequestReconstruction {
    if request_message.fields.is_empty() {
        return RequestReconstruction::Empty;
    }

    if path_fields.is_empty() {
        if matches!(body_shape, Some(RequestPartShape::VerbatimRequest { .. })) {
            return RequestReconstruction::VerbatimBody;
        }
        if matches!(query_shape, Some(RequestPartShape::VerbatimRequest { .. })) {
            return RequestReconstruction::VerbatimQuery;
        }
    }

    let fields = request_message
        .fields
        .iter()
        .map(|field| FieldAssignment {
            field: field.name.clone(),
            source: field_source(field, binding),
        })
        .collect();

    RequestReconstruction::FromParts { fields }
}

fn field_source(field: &Field, binding: &HttpBinding) -> FieldSource {
    if binding
        .path_variables
        .iter()
        .any(|path_field| path_field.as_ref() == field.name.as_ref())
    {
        FieldSource::Path
    } else if match &binding.body {
        HttpBody::Wildcard => true,
        HttpBody::Field(body_field) => body_field.as_ref() == field.name.as_ref(),
        HttpBody::None => false,
    } {
        FieldSource::Body
    } else {
        FieldSource::Query
    }
}

fn generated_dto_name(
    kind: GeneratedDtoKind,
    source_message: &Message,
    existing_count: usize,
    options: &CodegenOptions,
) -> SharedStr {
    let ordinal = if existing_count == 0 {
        String::new()
    } else {
        (existing_count + 1).to_string()
    };
    format!(
        "{}{}{}{}",
        source_message.name.as_ref(),
        kind.suffix(options),
        ordinal,
        options.type_suffix.as_ref()
    )
    .to_owned_opt()
}

fn same_shape_fields(left: &[ShapeField], right: &[ShapeField]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.field == right.field && left.rust_type == right.rust_type)
}

#[cfg(test)]
mod tests {
    use buffa::encoding::{Tag, WireType};
    use buffa::{MessageField, UnknownField, UnknownFieldData};
    use connectrpc_codegen::codegen::descriptor::{
        DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorProto,
        MethodDescriptorProto, MethodOptions, ServiceDescriptorProto,
        field_descriptor_proto::{Label, Type},
    };

    use super::{FieldSource, RequestPartShape, RequestReconstruction, plan_file_shapes};
    use crate::CodeGeneratorRequest;
    use crate::http::HTTP_EXTENSION_NUMBER;
    use crate::ir::build_ir;
    use crate::options::CodegenOptions;

    #[test]
    fn partitions_test_request_like_the_old_http_parser() {
        let ir = build_ir(&request(vec![test_file(vec![method(
            "DoTest",
            ".test.v1.TestRequest",
            http_rule(4, "/test/{data}/testing/{test_type}", Some("tester")),
        )])]))
        .unwrap();

        let shapes = plan_file_shapes(&ir, &ir.files[0], &CodegenOptions::default()).unwrap();
        let shape = &shapes.request_shapes[0];

        assert_eq!(
            shape
                .path_fields
                .iter()
                .map(|field| field.field.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["data", "test_type"]
        );
        assert!(matches!(
            &shape.body_shape,
            Some(RequestPartShape::ExistingMessage { field, rust_type })
                if field.field.name.as_ref() == "tester"
                    && rust_type.as_str() == "crate::proto::test::v1::Nested"
        ));
        assert!(shape.query_shape.is_none());
        assert!(matches!(
            &shape.reconstruction,
            RequestReconstruction::FromParts { fields }
                if fields.iter().map(|field| field.source).collect::<Vec<_>>()
                    == vec![FieldSource::Path, FieldSource::Path, FieldSource::Body]
        ));
    }

    #[test]
    fn generated_dto_names_are_deduplicated_by_kind_and_field_set() {
        let ir = build_ir(&request(vec![test_file(vec![
            method(
                "PatchOne",
                ".test.v1.TestRequest",
                http_rule(6, "/test/{data}", Some("*")),
            ),
            method(
                "PatchTwo",
                ".test.v1.TestRequest",
                http_rule(6, "/test/{data}", Some("*")),
            ),
            method(
                "PatchThree",
                ".test.v1.TestRequest",
                http_rule(6, "/test/{test_type}", Some("*")),
            ),
        ])]))
        .unwrap();

        let shapes = plan_file_shapes(&ir, &ir.files[0], &CodegenOptions::default()).unwrap();

        assert_eq!(
            shapes
                .generated_dtos
                .iter()
                .map(|dto| dto.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["TestRequestBody__", "TestRequestBody2__"]
        );
        assert_eq!(
            generated_body_name(&shapes.request_shapes[0]),
            generated_body_name(&shapes.request_shapes[1])
        );
        assert_ne!(
            generated_body_name(&shapes.request_shapes[0]),
            generated_body_name(&shapes.request_shapes[2])
        );
    }

    #[test]
    fn scalar_body_field_uses_generated_dto_unless_it_is_the_whole_request() {
        let ir = build_ir(&request(vec![test_file(vec![method(
            "PostScalar",
            ".test.v1.TestRequest",
            http_rule(4, "/test/{data}", Some("test_type")),
        )])]))
        .unwrap();

        let shapes = plan_file_shapes(&ir, &ir.files[0], &CodegenOptions::default()).unwrap();

        assert!(matches!(
            &shapes.request_shapes[0].body_shape,
            Some(RequestPartShape::GeneratedDto { name, fields, .. })
                if name.as_ref() == "TestRequestBody__"
                    && fields.iter().map(|field| field.field.name.as_ref()).collect::<Vec<_>>()
                        == vec!["test_type"]
        ));
    }

    #[test]
    fn scalar_body_field_uses_original_request_when_it_is_the_entire_request() {
        let ir = build_ir(&request(vec![single_field_file(vec![method(
            "PostName",
            ".test.v1.NameRequest",
            http_rule(4, "/test", Some("name")),
        )])]))
        .unwrap();

        let shapes = plan_file_shapes(&ir, &ir.files[0], &CodegenOptions::default()).unwrap();

        assert!(matches!(
            &shapes.request_shapes[0].body_shape,
            Some(RequestPartShape::VerbatimRequest { rust_type })
                if rust_type.as_str() == "crate::proto::test::v1::NameRequest"
        ));
        assert_eq!(
            shapes.request_shapes[0].reconstruction,
            RequestReconstruction::VerbatimBody
        );
    }

    #[test]
    fn body_wildcard_uses_original_request_without_query_fields() {
        let ir = build_ir(&request(vec![test_file(vec![method(
            "PatchAll",
            ".test.v1.TestRequest",
            http_rule(6, "/test", Some("*")),
        )])]))
        .unwrap();

        let shapes = plan_file_shapes(&ir, &ir.files[0], &CodegenOptions::default()).unwrap();

        assert!(matches!(
            &shapes.request_shapes[0].body_shape,
            Some(RequestPartShape::VerbatimRequest { rust_type })
                if rust_type.as_str() == "crate::proto::test::v1::TestRequest"
        ));
        assert!(shapes.request_shapes[0].query_shape.is_none());
        assert_eq!(
            shapes.request_shapes[0].reconstruction,
            RequestReconstruction::VerbatimBody
        );
    }

    #[test]
    fn no_body_leaves_remaining_fields_as_query_dto() {
        let ir = build_ir(&request(vec![test_file(vec![method(
            "GetOne",
            ".test.v1.TestRequest",
            http_rule(2, "/test/{data}", None),
        )])]))
        .unwrap();

        let shapes = plan_file_shapes(&ir, &ir.files[0], &CodegenOptions::default()).unwrap();

        assert!(matches!(
            &shapes.request_shapes[0].query_shape,
            Some(RequestPartShape::GeneratedDto { name, fields, .. })
                if name.as_ref() == "TestRequestQuery__"
                    && fields.iter().map(|field| field.field.name.as_ref()).collect::<Vec<_>>()
                        == vec!["test_type", "tester"]
        ));
    }

    #[test]
    fn google_protobuf_empty_plans_as_empty_request() {
        let ir = build_ir(&request(vec![
            FileDescriptorProto {
                name: Some("google/protobuf/empty.proto".into()),
                package: Some("google.protobuf".into()),
                message_type: vec![DescriptorProto {
                    name: Some("Empty".into()),
                    ..Default::default()
                }],
                ..Default::default()
            },
            FileDescriptorProto {
                name: Some("test/v1/empty.proto".into()),
                package: Some("test.v1".into()),
                service: vec![service(vec![method(
                    "Ping",
                    ".google.protobuf.Empty",
                    http_rule(2, "/ping", None),
                )])],
                ..Default::default()
            },
        ]))
        .unwrap();

        let shapes = plan_file_shapes(&ir, &ir.files[1], &CodegenOptions::default()).unwrap();

        assert!(shapes.request_shapes[0].path_fields.is_empty());
        assert!(shapes.request_shapes[0].query_shape.is_none());
        assert!(shapes.request_shapes[0].body_shape.is_none());
        assert_eq!(
            shapes.request_shapes[0].reconstruction,
            RequestReconstruction::Empty
        );
    }

    #[test]
    fn client_streaming_body_wildcard_uses_streamed_request_messages() {
        let ir = build_ir(&request(vec![test_file(vec![streaming_method(
            "ClientStream",
            ".test.v1.TestRequest",
            http_rule(4, "/test:stream", Some("*")),
            true,
            false,
        )])]))
        .unwrap();

        let shapes = plan_file_shapes(&ir, &ir.files[0], &CodegenOptions::default()).unwrap();

        assert!(matches!(
            &shapes.request_shapes[0].body_shape,
            Some(RequestPartShape::VerbatimRequest { rust_type })
                if rust_type.as_str() == "crate::proto::test::v1::TestRequest"
        ));
        assert!(shapes.request_shapes[0].path_fields.is_empty());
        assert!(shapes.request_shapes[0].query_shape.is_none());
        assert_eq!(
            shapes.request_shapes[0].reconstruction,
            RequestReconstruction::VerbatimBody
        );
    }

    #[test]
    fn client_streaming_rejects_path_parameters() {
        let ir = build_ir(&request(vec![test_file(vec![streaming_method(
            "ClientStream",
            ".test.v1.TestRequest",
            http_rule(4, "/test/{data}/stream", Some("*")),
            true,
            false,
        )])]))
        .unwrap();

        let err = plan_file_shapes(&ir, &ir.files[0], &CodegenOptions::default()).unwrap_err();

        assert!(err.to_string().contains("cannot bind path parameters"));
    }

    #[test]
    fn client_streaming_rejects_query_parameters() {
        let ir = build_ir(&request(vec![test_file(vec![streaming_method(
            "ClientStream",
            ".test.v1.TestRequest",
            http_rule(4, "/test:stream", None),
            true,
            false,
        )])]))
        .unwrap();

        let err = plan_file_shapes(&ir, &ir.files[0], &CodegenOptions::default()).unwrap_err();

        assert!(err.to_string().contains("cannot bind query parameters"));
    }

    #[test]
    fn client_streaming_rejects_field_body_bindings() {
        let ir = build_ir(&request(vec![single_field_file(vec![streaming_method(
            "ClientStream",
            ".test.v1.NameRequest",
            http_rule(4, "/test:stream", Some("name")),
            true,
            false,
        )])]))
        .unwrap();

        let err = plan_file_shapes(&ir, &ir.files[0], &CodegenOptions::default()).unwrap_err();

        assert!(err.to_string().contains("must use body: \"*\""));
    }

    fn generated_body_name(shape: &super::RequestShape) -> &str {
        match &shape.body_shape {
            Some(RequestPartShape::GeneratedDto { name, .. }) => name.as_ref(),
            _ => "",
        }
    }

    fn request(files: Vec<FileDescriptorProto>) -> CodeGeneratorRequest {
        CodeGeneratorRequest {
            proto_file: files,
            ..Default::default()
        }
    }

    fn test_file(methods: Vec<MethodDescriptorProto>) -> FileDescriptorProto {
        FileDescriptorProto {
            name: Some("test/v1/test.proto".into()),
            package: Some("test.v1".into()),
            message_type: vec![
                DescriptorProto {
                    name: Some("TestRequest".into()),
                    field: vec![
                        field("data", 1, Type::TYPE_STRING, None),
                        field("test_type", 2, Type::TYPE_ENUM, Some(".test.v1.Tester")),
                        field("tester", 8, Type::TYPE_MESSAGE, Some(".test.v1.Nested")),
                    ],
                    ..Default::default()
                },
                DescriptorProto {
                    name: Some("Nested".into()),
                    field: vec![field("data", 1, Type::TYPE_STRING, None)],
                    ..Default::default()
                },
            ],
            enum_type: vec![EnumDescriptorProto {
                name: Some("Tester".into()),
                ..Default::default()
            }],
            service: vec![service(methods)],
            ..Default::default()
        }
    }

    fn single_field_file(methods: Vec<MethodDescriptorProto>) -> FileDescriptorProto {
        FileDescriptorProto {
            name: Some("test/v1/name.proto".into()),
            package: Some("test.v1".into()),
            message_type: vec![DescriptorProto {
                name: Some("NameRequest".into()),
                field: vec![field("name", 1, Type::TYPE_STRING, None)],
                ..Default::default()
            }],
            service: vec![service(methods)],
            ..Default::default()
        }
    }

    fn service(methods: Vec<MethodDescriptorProto>) -> ServiceDescriptorProto {
        ServiceDescriptorProto {
            name: Some("TestService".into()),
            method: methods,
            ..Default::default()
        }
    }

    fn method(name: &str, input_type: &str, http_rule: Vec<u8>) -> MethodDescriptorProto {
        streaming_method(name, input_type, http_rule, false, false)
    }

    fn streaming_method(
        name: &str,
        input_type: &str,
        http_rule: Vec<u8>,
        client_streaming: bool,
        server_streaming: bool,
    ) -> MethodDescriptorProto {
        let mut options = MethodOptions::default();
        options.__buffa_unknown_fields.push(UnknownField {
            number: HTTP_EXTENSION_NUMBER,
            data: UnknownFieldData::LengthDelimited(http_rule),
        });

        MethodDescriptorProto {
            name: Some(name.into()),
            input_type: Some(input_type.into()),
            output_type: Some(".test.v1.TestReply".into()),
            client_streaming: Some(client_streaming),
            server_streaming: Some(server_streaming),
            options: MessageField::some(options),
            ..Default::default()
        }
    }

    fn field(
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
}
