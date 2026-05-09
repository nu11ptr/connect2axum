use std::path::PathBuf;

use connectrpc_codegen::plugin::{CodeGeneratorResponse, CodeGeneratorResponseFile};
use uni_error::UniError;

use crate::CodeGeneratorRequest;
use crate::error::{CodegenErrKind, CodegenResult};
use crate::ir::build_ir;

mod comments;
mod config;
mod document;
mod grpc_gateway;
mod model;
mod schema;
mod value;

use self::config::{DEFAULT_STREAMING_CONTENT_TYPE, DocConfig};
use self::document::merge_openapi_documents;
use self::grpc_gateway::{
    inject_go_packages, openapiv3_binary, openapiv3_parameter, run_openapiv3,
};

const DEFAULT_OUTPUT_FILE: &str = "openapi.json";

pub(crate) fn generate(request: &CodeGeneratorRequest) -> CodegenResult<CodeGeneratorResponse> {
    let options = OpenApiOptions::parse(request.parameter.as_deref())?;
    let config = options.load_config()?;
    let ir = build_ir(request)?;
    if !ir.files_to_generate.iter().any(|file_name| {
        ir.file(file_name.as_ref())
            .is_some_and(|file| file.has_http_bindings())
    }) {
        return Ok(CodeGeneratorResponse::default());
    }

    let mut child_request = request.clone();
    inject_go_packages(&mut child_request);
    child_request.parameter = Some(openapiv3_parameter(&options.openapiv3_options));

    let child = run_openapiv3(
        &openapiv3_binary(options.openapiv3_bin.as_deref())?,
        &child_request,
    )?;
    let supported_features = child.supported_features;
    let minimum_edition = child.minimum_edition;
    let maximum_edition = child.maximum_edition;
    let document = merge_openapi_documents(
        child.file,
        &ir,
        config.streaming_content_type(&options.streaming_content_type),
        &config,
    )?;
    let content = serde_json::to_string_pretty(&document).map_err(|err| {
        UniError::from_kind_context(
            CodegenErrKind::OpenApiInvalidDocument,
            format!("failed to serialize merged OpenAPI document: {err}"),
        )
    })? + "\n";

    Ok(CodeGeneratorResponse {
        file: vec![CodeGeneratorResponseFile {
            name: Some(options.output_file),
            content: Some(content),
            ..Default::default()
        }],
        supported_features,
        minimum_edition,
        maximum_edition,
        ..Default::default()
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenApiOptions {
    output_file: String,
    config_path: Option<PathBuf>,
    openapiv3_bin: Option<PathBuf>,
    openapiv3_options: Vec<String>,
    streaming_content_type: String,
}

impl Default for OpenApiOptions {
    fn default() -> Self {
        Self {
            output_file: DEFAULT_OUTPUT_FILE.to_owned(),
            config_path: None,
            openapiv3_bin: None,
            openapiv3_options: Vec::new(),
            streaming_content_type: DEFAULT_STREAMING_CONTENT_TYPE.to_owned(),
        }
    }
}

impl OpenApiOptions {
    fn parse(parameter: Option<&str>) -> CodegenResult<Self> {
        let mut options = Self::default();
        let Some(parameter) = parameter else {
            return Ok(options);
        };

        for raw_option in parameter.split(',') {
            let raw_option = raw_option.trim();
            if raw_option.is_empty() {
                continue;
            }

            let (name, value) = raw_option.split_once('=').ok_or_else(|| {
                invalid_option(format!(
                    "plugin option must use name=value syntax: {raw_option}"
                ))
            })?;
            let name = name.trim();
            let value = value.trim();
            if value.is_empty() {
                return Err(invalid_option(format!("{name} cannot be empty")));
            }

            match name {
                "output" => options.output_file = value.to_owned(),
                "config" => options.config_path = Some(PathBuf::from(value)),
                "openapiv3_bin" => options.openapiv3_bin = Some(PathBuf::from(value)),
                "openapiv3_opt" => options.openapiv3_options.push(value.to_owned()),
                "streaming_content_type" => options.streaming_content_type = value.to_owned(),
                _ => {
                    return Err(UniError::from_kind_context(
                        CodegenErrKind::UnknownPluginOption,
                        format!("unknown plugin option: {name}"),
                    ));
                }
            }
        }

        Ok(options)
    }

    fn load_config(&self) -> CodegenResult<DocConfig> {
        let Some(path) = self.config_path.as_ref() else {
            return Ok(DocConfig::default());
        };
        DocConfig::from_path(path)
    }
}

fn invalid_option(context: String) -> uni_error::UniError<CodegenErrKind> {
    UniError::from_kind_context(CodegenErrKind::InvalidPluginOption, context)
}

#[cfg(test)]
mod tests {
    use buffa::MessageField;
    use connectrpc_codegen::codegen::descriptor::{
        DescriptorProto, FileDescriptorProto, MethodDescriptorProto, ServiceDescriptorProto,
        field_descriptor_proto::{Label, Type},
    };
    use serde_json::json;

    use super::OpenApiOptions;
    use super::config::{DocConfig, HeaderConfig, InfoConfig};
    use super::document::{
        add_connect_error_response, apply_config, merge_document, merge_openapi_documents,
        patch_streaming_operations,
    };
    use super::grpc_gateway::inject_go_packages;
    use crate::ir::{
        DescriptorIr, Field, FieldKind, FieldLabel, HttpBinding, HttpBody, HttpVerb, Message,
        Method, ProtoFile, Service,
    };

    #[test]
    fn parses_openapi_options() {
        let options = OpenApiOptions::parse(Some(
            "output=docs/api.json,config=openapi.yaml,openapiv3_bin=/tmp/protoc-gen-openapiv3,openapiv3_opt=enum_type=string,streaming_content_type=application/x-custom",
        ))
        .unwrap();

        assert_eq!(options.output_file, "docs/api.json");
        assert_eq!(
            options
                .config_path
                .as_deref()
                .and_then(|path| path.to_str()),
            Some("openapi.yaml")
        );
        assert_eq!(
            options
                .openapiv3_bin
                .as_deref()
                .and_then(|path| path.to_str()),
            Some("/tmp/protoc-gen-openapiv3")
        );
        assert_eq!(options.openapiv3_options, vec!["enum_type=string"]);
        assert_eq!(options.streaming_content_type, "application/x-custom");
    }

    #[test]
    fn injects_synthetic_go_package_when_missing() {
        let mut request = crate::CodeGeneratorRequest {
            proto_file: vec![FileDescriptorProto {
                name: Some("hello/v1/hello.proto".into()),
                package: Some("hello.v1".into()),
                ..Default::default()
            }],
            ..Default::default()
        };

        inject_go_packages(&mut request);

        assert_eq!(
            request.proto_file[0]
                .options
                .as_option()
                .and_then(|options| options.go_package.as_deref()),
            Some("connect2axum.local/gen/hello/v1/hello;v1")
        );
    }

    #[test]
    fn merge_rejects_conflicting_paths() {
        let mut target = json!({
            "openapi": "3.1.0",
            "info": { "title": "one", "version": "1" },
            "paths": { "/hello": { "get": { "operationId": "one" } } }
        });
        let source = json!({
            "openapi": "3.1.0",
            "paths": { "/hello": { "get": { "operationId": "two" } } }
        });

        let err = merge_document(&mut target, source, "two.openapi.json").unwrap_err();

        assert!(err.to_string().contains("conflicting OpenAPI key"));
    }

    #[test]
    fn applies_config_headers_security_and_info() {
        let mut document = json!({
            "openapi": "3.1.0",
            "info": { "title": "old", "version": "0" },
            "paths": {
                "/hello": {
                    "post": {
                        "operationId": "SayHello",
                        "responses": { "200": { "description": "ok" } }
                    }
                }
            },
            "components": {}
        });
        let mut security_schemes = std::collections::BTreeMap::new();
        security_schemes.insert(
            "BearerAuth".to_owned(),
            json!({ "type": "http", "scheme": "bearer" }),
        );
        let config = DocConfig {
            info: InfoConfig {
                title: Some("Simple API".to_owned()),
                version: Some("1.2.3".to_owned()),
                ..Default::default()
            },
            servers: vec![json!({ "url": "http://127.0.0.1:8000/v1" })],
            security_schemes,
            security: Some(json!([{ "BearerAuth": [] }])),
            headers: vec![HeaderConfig {
                name: "X-Request-Id".to_owned(),
                required: false,
                description: None,
                schema: json!({ "type": "string" }),
            }],
            ..Default::default()
        };

        apply_config(&mut document, &config).unwrap();

        assert_eq!(document["info"]["title"], "Simple API");
        assert_eq!(document["servers"][0]["url"], "http://127.0.0.1:8000/v1");
        assert_eq!(
            document["components"]["securitySchemes"]["BearerAuth"]["scheme"],
            "bearer"
        );
        assert_eq!(
            document["paths"]["/hello"]["post"]["parameters"][0]["name"],
            "X-Request-Id"
        );
    }

    #[test]
    fn patches_streaming_operations_to_ndjson() {
        let mut document = json!({
            "openapi": "3.1.0",
            "info": { "title": "streaming", "version": "1" },
            "paths": {
                "/chat": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Request" }
                                }
                            }
                        },
                        "responses": {
                            "200": {
                                "content": {
                                    "application/json": {
                                        "schema": { "$ref": "#/components/schemas/Reply" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        let ir = streaming_ir(true, true);

        patch_streaming_operations(&mut document, &ir, "application/x-ndjson").unwrap();

        assert!(
            document["paths"]["/chat"]["post"]["requestBody"]["content"]
                .get("application/x-ndjson")
                .is_some()
        );
        assert!(
            document["paths"]["/chat"]["post"]["responses"]["200"]["content"]
                .get("application/x-ndjson")
                .is_some()
        );
        assert_eq!(
            document["paths"]["/chat"]["post"]["x-connect2axum-streaming"]["framing"],
            "ndjson"
        );
    }

    #[test]
    fn adds_connect_error_default_response() {
        let mut document = json!({
            "openapi": "3.1.0",
            "info": { "title": "simple", "version": "1" },
            "paths": {
                "/hello": {
                    "post": {
                        "responses": { "200": { "description": "ok" } }
                    }
                }
            },
            "components": {}
        });

        add_connect_error_response(&mut document).unwrap();

        assert_eq!(
            document["paths"]["/hello"]["post"]["responses"]["default"]["$ref"],
            "#/components/responses/connect2axum.ConnectError"
        );
        assert!(
            document["components"]["schemas"]
                .get("connect2axum.ConnectError")
                .is_some()
        );
    }

    #[test]
    fn merges_multiple_child_documents_and_validates_with_oas3() {
        let files = vec![
            child_file(
                "one.openapi.json",
                json!({
                    "openapi": "3.1.0",
                    "info": { "title": "one", "version": "1" },
                    "paths": { "/one": { "get": { "responses": { "200": { "description": "ok" } } } } },
                    "components": { "schemas": { "One": { "type": "object" } } }
                }),
            ),
            child_file(
                "two.openapi.json",
                json!({
                    "openapi": "3.1.0",
                    "info": { "title": "two", "version": "1" },
                    "paths": { "/two": { "post": { "responses": { "200": { "description": "ok" } } } } },
                    "components": { "schemas": { "Two": { "type": "object" } } }
                }),
            ),
        ];
        let document = merge_openapi_documents(
            files,
            &DescriptorIr::default(),
            "application/x-ndjson",
            &DocConfig::default(),
        )
        .unwrap();

        assert!(document["paths"].get("/one").is_some());
        assert!(document["paths"].get("/two").is_some());
        assert!(document["components"]["schemas"].get("One").is_some());
        assert!(document["components"]["schemas"].get("Two").is_some());
    }

    fn child_file(
        name: &str,
        content: serde_json::Value,
    ) -> connectrpc_codegen::plugin::CodeGeneratorResponseFile {
        connectrpc_codegen::plugin::CodeGeneratorResponseFile {
            name: Some(name.to_owned()),
            content: Some(serde_json::to_string(&content).unwrap()),
            ..Default::default()
        }
    }

    fn streaming_ir(client_streaming: bool, server_streaming: bool) -> DescriptorIr {
        DescriptorIr {
            files_to_generate: vec!["streaming.proto".into()],
            files: vec![ProtoFile {
                name: "streaming.proto".into(),
                package: "streaming.v1".into(),
                messages: vec![
                    Message {
                        name: "Request".into(),
                        full_name: ".streaming.v1.Request".into(),
                        comments: Default::default(),
                        fields: vec![Field {
                            name: "message".into(),
                            json_name: "message".into(),
                            number: Some(1),
                            label: Some(FieldLabel::Optional),
                            kind: FieldKind::String,
                            comments: Default::default(),
                        }],
                        messages: Vec::new(),
                    },
                    Message {
                        name: "Reply".into(),
                        full_name: ".streaming.v1.Reply".into(),
                        comments: Default::default(),
                        fields: Vec::new(),
                        messages: Vec::new(),
                    },
                ],
                services: vec![Service {
                    name: "ChatService".into(),
                    full_name: ".streaming.v1.ChatService".into(),
                    comments: Default::default(),
                    methods: vec![Method {
                        name: "Chat".into(),
                        full_name: ".streaming.v1.ChatService.Chat".into(),
                        input_type: ".streaming.v1.Request".into(),
                        output_type: ".streaming.v1.Reply".into(),
                        client_streaming,
                        server_streaming,
                        comments: Default::default(),
                        http: Some(HttpBinding {
                            verb: HttpVerb::Post,
                            path: "/chat".into(),
                            body: HttpBody::Wildcard,
                            path_variables: Vec::new(),
                        }),
                    }],
                }],
            }],
            descriptor_files: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn _descriptor_method() -> MethodDescriptorProto {
        MethodDescriptorProto {
            name: Some("Chat".into()),
            input_type: Some(".streaming.v1.Request".into()),
            output_type: Some(".streaming.v1.Reply".into()),
            client_streaming: Some(true),
            server_streaming: Some(true),
            options: MessageField::none(),
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    fn _descriptor_file() -> FileDescriptorProto {
        FileDescriptorProto {
            name: Some("streaming.proto".into()),
            package: Some("streaming.v1".into()),
            message_type: vec![DescriptorProto {
                name: Some("Request".into()),
                field: vec![
                    connectrpc_codegen::codegen::descriptor::FieldDescriptorProto {
                        name: Some("message".into()),
                        number: Some(1),
                        label: Some(Label::LABEL_OPTIONAL),
                        r#type: Some(Type::TYPE_STRING),
                        json_name: Some("message".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            service: vec![ServiceDescriptorProto {
                name: Some("ChatService".into()),
                method: vec![_descriptor_method()],
                ..Default::default()
            }],
            ..Default::default()
        }
    }
}
