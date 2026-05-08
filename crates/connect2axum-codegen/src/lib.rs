//! Protoc/Buf code generation for `connect2axum`.
//!

mod error;
mod http;
mod ir;
mod options;
mod resolver;
mod rest;
mod shape;
mod ws;

pub use connectrpc_codegen::plugin::{CodeGeneratorRequest, CodeGeneratorResponse};
pub use error::{CodegenErrKind, CodegenResult};
pub use ir::{
    CommentSet, DescriptorIr, Field, FieldKind, FieldLabel, HttpBinding, HttpBody, HttpVerb,
    Message, Method, ProtoFile, Service, build_ir,
};
pub use options::{CodegenOptions, ServiceState};
pub use resolver::{RustPath, TypeResolver};
pub use shape::{
    FieldAssignment, FieldSource, FileShapes, GeneratedDto, GeneratedDtoKind, RequestPartShape,
    RequestReconstruction, RequestShape, ShapeField, plan_file_shapes,
};

/// Generate a REST protoc plugin response for a request.
///
/// Errors are returned through the protoc plugin error field so `buf generate`
/// and `protoc` can display them as compiler-plugin failures.
#[must_use]
pub fn generate_rest(request: &CodeGeneratorRequest) -> CodeGeneratorResponse {
    match try_generate_rest(request) {
        Ok(response) => response,
        Err(err) => CodeGeneratorResponse {
            error: Some(err.to_string()),
            ..Default::default()
        },
    }
}

/// Generate a REST protoc plugin response, returning typed project errors.
pub fn try_generate_rest(request: &CodeGeneratorRequest) -> CodegenResult<CodeGeneratorResponse> {
    let options = CodegenOptions::parse(request.parameter.as_deref())?;
    let ir = build_ir(request)?;
    let files = request
        .file_to_generate
        .iter()
        .map(|file_name| rest::generate_file(&ir, file_name, &options))
        .collect::<CodegenResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    Ok(CodeGeneratorResponse {
        file: files,
        ..Default::default()
    })
}

/// Generate a WebSocket protoc plugin response for a request.
///
/// Errors are returned through the protoc plugin error field so `buf generate`
/// and `protoc` can display them as compiler-plugin failures.
#[must_use]
pub fn generate_ws(request: &CodeGeneratorRequest) -> CodeGeneratorResponse {
    match try_generate_ws(request) {
        Ok(response) => response,
        Err(err) => CodeGeneratorResponse {
            error: Some(err.to_string()),
            ..Default::default()
        },
    }
}

/// Generate a WebSocket protoc plugin response, returning typed project errors.
pub fn try_generate_ws(request: &CodeGeneratorRequest) -> CodegenResult<CodeGeneratorResponse> {
    let options = CodegenOptions::parse(request.parameter.as_deref())?;
    let ir = build_ir(request)?;
    let files = request
        .file_to_generate
        .iter()
        .map(|file_name| ws::generate_file(&ir, file_name, &options))
        .collect::<CodegenResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    Ok(CodeGeneratorResponse {
        file: files,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use buffa::Message as _;
    use buffa::encoding::{Tag, WireType};
    use buffa::{MessageField, UnknownField, UnknownFieldData};
    use connectrpc_codegen::codegen::descriptor::{
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto, MethodDescriptorProto,
        MethodOptions, ServiceDescriptorProto,
        field_descriptor_proto::{Label, Type},
    };

    use super::{CodeGeneratorRequest, CodeGeneratorResponse, generate_rest, try_generate_rest};

    #[test]
    fn empty_request_generates_empty_response() {
        let request = CodeGeneratorRequest::default();

        let response = generate_rest(&request);

        assert!(response.file.is_empty());
        assert!(response.error.is_none());
    }

    #[test]
    fn unknown_option_generates_plugin_error_response() {
        let request = CodeGeneratorRequest {
            parameter: Some("surprise=true".into()),
            ..Default::default()
        };

        let response = generate_rest(&request);

        assert!(response.file.is_empty());
        assert!(
            response
                .error
                .as_deref()
                .is_some_and(|err| err.contains("unknown plugin option: surprise"))
        );
    }

    #[test]
    fn generates_deterministic_file_names_for_files_with_http_bindings() {
        let request = CodeGeneratorRequest {
            file_to_generate: vec!["hello/v1/hello.proto".into(), "echo.proto".into()],
            proto_file: vec![
                test_file("hello/v1/hello.proto", "hello.v1", true),
                test_file("echo.proto", "echo.v1", true),
            ],
            ..Default::default()
        };

        let response = try_generate_rest(&request).unwrap();

        let names = response
            .file
            .iter()
            .map(|file| file.name.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                Some("hello/v1/hello.connect2rest.rs"),
                Some("echo.connect2rest.rs")
            ]
        );
    }

    #[test]
    fn skips_files_without_http_bindings() {
        let request = CodeGeneratorRequest {
            file_to_generate: vec!["hello/v1/hello.proto".into()],
            proto_file: vec![test_file("hello/v1/hello.proto", "hello.v1", false)],
            ..Default::default()
        };

        let response = try_generate_rest(&request).unwrap();

        assert!(response.file.is_empty());
    }

    #[test]
    fn missing_file_to_generate_is_a_typed_error() {
        let request = CodeGeneratorRequest {
            file_to_generate: vec!["missing.proto".into()],
            proto_file: vec![],
            ..Default::default()
        };

        let err = try_generate_rest(&request).unwrap_err();

        assert!(
            err.to_string()
                .contains("file_to_generate \"missing.proto\" was not present in proto_file")
        );
    }

    #[test]
    fn plugin_protocol_messages_round_trip() {
        let request = CodeGeneratorRequest::default();
        let request_bytes = request.encode_to_vec();
        let decoded_request =
            CodeGeneratorRequest::decode_from_slice(&request_bytes).expect("request decodes");

        let response = generate_rest(&decoded_request);
        let response_bytes = response.encode_to_vec();
        let decoded_response =
            CodeGeneratorResponse::decode_from_slice(&response_bytes).expect("response decodes");

        assert!(decoded_response.file.is_empty());
        assert!(decoded_response.error.is_none());
    }

    fn test_file(name: &str, package: &str, with_http_binding: bool) -> FileDescriptorProto {
        FileDescriptorProto {
            name: Some(name.into()),
            package: Some(package.into()),
            message_type: vec![
                DescriptorProto {
                    name: Some("HelloRequest".into()),
                    field: vec![FieldDescriptorProto {
                        name: Some("name".into()),
                        number: Some(1),
                        label: Some(Label::LABEL_OPTIONAL),
                        r#type: Some(Type::TYPE_STRING),
                        json_name: Some("name".into()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                DescriptorProto {
                    name: Some("HelloResponse".into()),
                    ..Default::default()
                },
            ],
            service: vec![ServiceDescriptorProto {
                name: Some("HelloService".into()),
                method: vec![MethodDescriptorProto {
                    name: Some("SayHello".into()),
                    input_type: Some(format!(".{package}.HelloRequest")),
                    output_type: Some(format!(".{package}.HelloResponse")),
                    options: if with_http_binding {
                        method_options()
                    } else {
                        MessageField::none()
                    },
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn method_options() -> MessageField<MethodOptions> {
        let mut rule = Vec::new();
        Tag::new(2, WireType::LengthDelimited).encode(&mut rule);
        buffa::types::encode_string("/hello/{name}", &mut rule);

        let mut options = MethodOptions::default();
        options.__buffa_unknown_fields.push(UnknownField {
            number: 72_295_728,
            data: UnknownFieldData::LengthDelimited(rule),
        });
        MessageField::some(options)
    }
}
