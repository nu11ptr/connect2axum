use connectrpc_codegen::plugin::CodeGeneratorResponseFile;
use serde_json::{Map, Value, json};
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};
use crate::ir::{DescriptorIr, HttpVerb, ProtoFile};
use crate::options::CodegenOptions;
use crate::shape::{RequestPartShape, ShapeField, plan_file_shapes};

use super::config::{DocConfig, HeaderConfig, InfoConfig};
use super::model::validate_document;
use super::schema::generated_dto_schema;
use super::value::{ensure_array_at, ensure_nested_object, ensure_object_at, merge_named_values};

const DEFAULT_OPENAPI_FILE_NAME: &str = "openapi.json";
const OPENAPI_JSON_CONTENT_TYPE: &str = "application/json";
const CONNECT_ERROR_SCHEMA: &str = "connect2axum.ConnectError";
const CONNECT_ERROR_RESPONSE: &str = "connect2axum.ConnectError";

pub(crate) fn merge_openapi_documents(
    files: Vec<CodeGeneratorResponseFile>,
    ir: &DescriptorIr,
    streaming_content_type: &str,
    config: &DocConfig,
) -> CodegenResult<Value> {
    let mut merged = json!({
        "openapi": "3.1.0",
        "info": {
            "title": "API",
            "version": "1.0.0"
        },
        "paths": {},
        "components": {}
    });
    let mut saw_document = false;
    let emitted_names = files
        .iter()
        .map(|file| {
            format!(
                "{} ({} bytes)",
                file.name.as_deref().unwrap_or("<append>"),
                file.content.as_deref().map(str::len).unwrap_or(0)
            )
        })
        .collect::<Vec<_>>();

    for file in files {
        let name = file.name.as_deref().unwrap_or(DEFAULT_OPENAPI_FILE_NAME);
        let parse_name = if name.is_empty() {
            DEFAULT_OPENAPI_FILE_NAME
        } else {
            name
        };
        if !(parse_name.ends_with(".json")
            || parse_name.ends_with(".yaml")
            || parse_name.ends_with(".yml"))
        {
            continue;
        }
        let Some(content) = file.content.as_deref() else {
            continue;
        };

        let document = parse_openapi_document(parse_name, content)?;
        saw_document = true;
        merge_document(&mut merged, document, parse_name)?;
    }

    if !saw_document {
        return Err(UniError::from_kind_context(
            CodegenErrKind::OpenApiInvalidDocument,
            format!(
                "protoc-gen-openapiv3 did not emit any OpenAPI JSON or YAML files; files_to_generate: {}; descriptor files: {}; emitted files: {}",
                ir.files_to_generate
                    .iter()
                    .map(|name| name.as_ref())
                    .collect::<Vec<_>>()
                    .join(", "),
                ir.files
                    .iter()
                    .map(|file| file.name.as_ref())
                    .collect::<Vec<_>>()
                    .join(", "),
                emitted_names.join(", "),
            ),
        ));
    }

    apply_config(&mut merged, config)?;
    patch_generated_body_shapes(&mut merged, ir)?;
    patch_streaming_operations(&mut merged, ir, streaming_content_type)?;
    if config.add_default_error_response() {
        add_connect_error_response(&mut merged)?;
    }
    validate_document(&merged)?;

    Ok(merged)
}

fn parse_openapi_document(name: &str, content: &str) -> CodegenResult<Value> {
    if name.ends_with(".yaml") || name.ends_with(".yml") {
        serde_yaml::from_str(content).map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiInvalidDocument,
                format!("failed to parse {name} as YAML: {err}"),
            )
        })
    } else {
        serde_json::from_str(content).map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiInvalidDocument,
                format!("failed to parse {name} as JSON: {err}"),
            )
        })
    }
}

pub(crate) fn merge_document(
    target: &mut Value,
    source: Value,
    source_name: &str,
) -> CodegenResult<()> {
    if let Some(openapi) = source.get("openapi") {
        target["openapi"] = openapi.clone();
    }
    merge_info(target, &source);
    merge_paths(target, &source, source_name)?;
    merge_components(target, &source, source_name)?;
    merge_tags(target, &source, source_name)?;
    merge_array_by_value(target, &source, "servers");
    merge_array_by_value(target, &source, "security");
    Ok(())
}

fn merge_info(target: &mut Value, source: &Value) {
    let Some(info) = source.get("info").and_then(Value::as_object) else {
        return;
    };
    let target_info = ensure_object_at(target, "info");
    for (key, value) in info {
        target_info
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}

fn merge_paths(target: &mut Value, source: &Value, source_name: &str) -> CodegenResult<()> {
    let Some(source_paths) = source.get("paths").and_then(Value::as_object) else {
        return Ok(());
    };
    let target_paths = ensure_object_at(target, "paths");

    for (path, source_path_item) in source_paths {
        let Some(source_path_item_object) = source_path_item.as_object() else {
            target_paths
                .entry(path.clone())
                .or_insert_with(|| source_path_item.clone());
            continue;
        };
        let target_path_item = target_paths
            .entry(path.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        let target_path_item_object = target_path_item.as_object_mut().ok_or_else(|| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiMergeConflict,
                format!("path item {path:?} was not an object while merging {source_name}"),
            )
        })?;
        merge_named_values(
            target_path_item_object,
            source_path_item_object,
            &format!("{source_name}:paths.{path}"),
        )?;
    }

    Ok(())
}

fn merge_components(target: &mut Value, source: &Value, source_name: &str) -> CodegenResult<()> {
    let Some(source_components) = source.get("components").and_then(Value::as_object) else {
        return Ok(());
    };
    let target_components = ensure_object_at(target, "components");

    for (section, source_value) in source_components {
        let Some(source_object) = source_value.as_object() else {
            target_components
                .entry(section.clone())
                .or_insert_with(|| source_value.clone());
            continue;
        };
        let target_section = target_components
            .entry(section.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        let target_object = target_section.as_object_mut().ok_or_else(|| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiMergeConflict,
                format!("components.{section} was not an object while merging {source_name}"),
            )
        })?;
        merge_named_values(
            target_object,
            source_object,
            &format!("{source_name}:components.{section}"),
        )?;
    }

    Ok(())
}

fn merge_tags(target: &mut Value, source: &Value, source_name: &str) -> CodegenResult<()> {
    let Some(source_tags) = source.get("tags").and_then(Value::as_array) else {
        return Ok(());
    };
    let target_tags = ensure_array_at(target, "tags")?;

    for source_tag in source_tags {
        let Some(source_name_value) = source_tag.get("name").and_then(Value::as_str) else {
            if !target_tags.iter().any(|tag| tag == source_tag) {
                target_tags.push(source_tag.clone());
            }
            continue;
        };

        let existing = target_tags
            .iter_mut()
            .find(|tag| tag.get("name").and_then(Value::as_str) == Some(source_name_value));
        if let Some(existing) = existing {
            merge_tag(existing, source_tag, source_name)?;
        } else {
            target_tags.push(source_tag.clone());
        }
    }

    Ok(())
}

fn merge_tag(target: &mut Value, source: &Value, source_name: &str) -> CodegenResult<()> {
    let Some(source_object) = source.as_object() else {
        return Ok(());
    };
    let Some(target_object) = target.as_object_mut() else {
        return Err(UniError::from_kind_context(
            CodegenErrKind::OpenApiMergeConflict,
            format!("tag was not an object while merging {source_name}"),
        ));
    };

    for (key, value) in source_object {
        match target_object.get(key) {
            Some(existing) if existing != value => {
                if key != "description" {
                    return Err(UniError::from_kind_context(
                        CodegenErrKind::OpenApiMergeConflict,
                        format!("conflicting tag field {key:?} while merging {source_name}"),
                    ));
                }
            }
            Some(_) => {}
            None => {
                target_object.insert(key.clone(), value.clone());
            }
        }
    }

    Ok(())
}

fn merge_array_by_value(target: &mut Value, source: &Value, field: &str) {
    let Some(source_values) = source.get(field).and_then(Value::as_array) else {
        return;
    };
    let target_values =
        ensure_array_at(target, field).expect("root array field should be settable");
    for value in source_values {
        if !target_values.iter().any(|existing| existing == value) {
            target_values.push(value.clone());
        }
    }
}

pub(crate) fn apply_config(document: &mut Value, config: &DocConfig) -> CodegenResult<()> {
    apply_info_config(document, &config.info);

    if !config.servers.is_empty() {
        document["servers"] = Value::Array(config.servers.clone());
    }

    if let Some(security) = config.security.as_ref() {
        document["security"] = security.clone();
    }

    if !config.security_schemes.is_empty() {
        let components = ensure_object_at(document, "components");
        let schemes = ensure_nested_object(components, "securitySchemes")?;
        for (name, scheme) in &config.security_schemes {
            schemes.insert(name.clone(), scheme.clone());
        }
    }

    if !config.headers.is_empty() {
        add_headers_to_operations(document, &config.headers)?;
    }

    Ok(())
}

fn apply_info_config(document: &mut Value, info: &InfoConfig) {
    let target = ensure_object_at(document, "info");
    if let Some(title) = info.title.as_ref() {
        target.insert("title".to_owned(), Value::String(title.clone()));
    }
    if let Some(version) = info.version.as_ref() {
        target.insert("version".to_owned(), Value::String(version.clone()));
    }
    if let Some(description) = info.description.as_ref() {
        target.insert("description".to_owned(), Value::String(description.clone()));
    }
    if let Some(terms) = info.terms_of_service.as_ref() {
        target.insert("termsOfService".to_owned(), Value::String(terms.clone()));
    }
}

fn add_headers_to_operations(document: &mut Value, headers: &[HeaderConfig]) -> CodegenResult<()> {
    let Some(paths) = document.get_mut("paths").and_then(Value::as_object_mut) else {
        return Ok(());
    };

    for path_item in paths.values_mut() {
        let Some(path_item) = path_item.as_object_mut() else {
            continue;
        };
        for &method in http_method_names() {
            let Some(operation) = path_item.get_mut(method).and_then(Value::as_object_mut) else {
                continue;
            };
            let parameters = operation
                .entry("parameters")
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .ok_or_else(|| {
                    UniError::from_kind_context(
                        CodegenErrKind::OpenApiInvalidDocument,
                        "operation parameters was not an array",
                    )
                })?;

            for header in headers {
                if header.name.trim().is_empty() {
                    return Err(UniError::from_kind_context(
                        CodegenErrKind::OpenApiInvalidDocument,
                        "configured OpenAPI header name cannot be empty",
                    ));
                }
                if parameters.iter().any(|parameter| {
                    parameter.get("in").and_then(Value::as_str) == Some("header")
                        && parameter.get("name").and_then(Value::as_str)
                            == Some(header.name.as_str())
                }) {
                    continue;
                }
                parameters.push(header_parameter(header));
            }
        }
    }

    Ok(())
}

fn header_parameter(header: &HeaderConfig) -> Value {
    let mut parameter = Map::new();
    parameter.insert("name".to_owned(), Value::String(header.name.clone()));
    parameter.insert("in".to_owned(), Value::String("header".to_owned()));
    parameter.insert("required".to_owned(), Value::Bool(header.required));
    parameter.insert("schema".to_owned(), header.schema.clone());
    if let Some(description) = header.description.as_ref() {
        parameter.insert("description".to_owned(), Value::String(description.clone()));
    }
    Value::Object(parameter)
}

fn patch_generated_body_shapes(document: &mut Value, ir: &DescriptorIr) -> CodegenResult<()> {
    let codegen_options = CodegenOptions::default();

    for file_name in &ir.files_to_generate {
        let Some(file) = ir.file(file_name.as_ref()) else {
            continue;
        };
        patch_file_generated_body_shapes(document, ir, file, &codegen_options)?;
    }

    Ok(())
}

fn patch_file_generated_body_shapes(
    document: &mut Value,
    ir: &DescriptorIr,
    file: &ProtoFile,
    codegen_options: &CodegenOptions,
) -> CodegenResult<()> {
    let shapes = plan_file_shapes(ir, file, codegen_options)?;

    for shape in &shapes.request_shapes {
        let Some(RequestPartShape::GeneratedDto { fields, .. }) = shape.body_shape.as_ref() else {
            continue;
        };
        let Some(method) = file
            .services
            .iter()
            .flat_map(|service| service.methods.iter())
            .find(|method| method.full_name == shape.method)
        else {
            continue;
        };
        let Some(binding) = method.http.as_ref() else {
            continue;
        };
        let method_name = openapi_method_name(binding.verb);
        let Some(operation) = operation_mut(document, binding.path.as_ref(), method_name)? else {
            continue;
        };
        replace_request_body_schema(operation, fields)?;
    }

    Ok(())
}

fn replace_request_body_schema(
    operation: &mut Map<String, Value>,
    fields: &[ShapeField],
) -> CodegenResult<()> {
    let Some(content) = nested_object_mut(operation, &["requestBody", "content"])? else {
        return Ok(());
    };
    let Some(media_type) = content
        .get_mut(OPENAPI_JSON_CONTENT_TYPE)
        .and_then(Value::as_object_mut)
    else {
        return Ok(());
    };

    media_type.insert("schema".to_owned(), generated_dto_schema(fields));
    Ok(())
}

pub(crate) fn patch_streaming_operations(
    document: &mut Value,
    ir: &DescriptorIr,
    streaming_content_type: &str,
) -> CodegenResult<()> {
    for file_name in &ir.files_to_generate {
        let Some(file) = ir.file(file_name.as_ref()) else {
            continue;
        };
        for service in &file.services {
            for method in &service.methods {
                if !method.client_streaming && !method.server_streaming {
                    continue;
                }
                let Some(binding) = method.http.as_ref() else {
                    continue;
                };
                let method_name = openapi_method_name(binding.verb);
                let Some(operation) = operation_mut(document, binding.path.as_ref(), method_name)?
                else {
                    continue;
                };

                if method.client_streaming {
                    replace_content_type(
                        operation,
                        &["requestBody", "content"],
                        streaming_content_type,
                    )?;
                }
                if method.server_streaming {
                    replace_content_type(
                        operation,
                        &["responses", "200", "content"],
                        streaming_content_type,
                    )?;
                }
                operation.insert(
                    "x-connect2axum-streaming".to_owned(),
                    json!({
                        "framing": "ndjson",
                        "request": method.client_streaming,
                        "response": method.server_streaming
                    }),
                );
            }
        }
    }

    Ok(())
}

fn operation_mut<'a>(
    document: &'a mut Value,
    path: &str,
    method: &str,
) -> CodegenResult<Option<&'a mut Map<String, Value>>> {
    let Some(paths) = document.get_mut("paths").and_then(Value::as_object_mut) else {
        return Ok(None);
    };
    let Some(path_item) = paths.get_mut(path).and_then(Value::as_object_mut) else {
        return Ok(None);
    };
    path_item
        .get_mut(method)
        .map(|operation| {
            operation.as_object_mut().ok_or_else(|| {
                UniError::from_kind_context(
                    CodegenErrKind::OpenApiInvalidDocument,
                    format!("OpenAPI operation {method} {path} was not an object"),
                )
            })
        })
        .transpose()
}

fn replace_content_type(
    operation: &mut Map<String, Value>,
    path: &[&str],
    content_type: &str,
) -> CodegenResult<()> {
    let Some(content) = nested_object_mut(operation, path)? else {
        return Ok(());
    };

    if content.contains_key(content_type) {
        return Ok(());
    }

    let media_type = content
        .remove(OPENAPI_JSON_CONTENT_TYPE)
        .or_else(|| content.values().next().cloned());
    if let Some(mut media_type) = media_type {
        if let Some(media_type_object) = media_type.as_object_mut() {
            media_type_object.insert(
                "x-connect2axum-line-delimited".to_owned(),
                Value::Bool(true),
            );
        }
        content.clear();
        content.insert(content_type.to_owned(), media_type);
    }

    Ok(())
}

fn nested_object_mut<'a>(
    root: &'a mut Map<String, Value>,
    path: &[&str],
) -> CodegenResult<Option<&'a mut Map<String, Value>>> {
    let mut current = root;
    for (index, segment) in path.iter().enumerate() {
        let Some(value) = current.get_mut(*segment) else {
            return Ok(None);
        };
        if index == path.len() - 1 {
            return value.as_object_mut().map(Some).ok_or_else(|| {
                UniError::from_kind_context(
                    CodegenErrKind::OpenApiInvalidDocument,
                    format!("OpenAPI path {} was not an object", path.join(".")),
                )
            });
        }
        current = value.as_object_mut().ok_or_else(|| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiInvalidDocument,
                format!("OpenAPI path {} was not an object", path.join(".")),
            )
        })?;
    }
    Ok(Some(current))
}

pub(crate) fn add_connect_error_response(document: &mut Value) -> CodegenResult<()> {
    let components = ensure_object_at(document, "components");
    let schemas = ensure_nested_object(components, "schemas")?;
    schemas
        .entry(CONNECT_ERROR_SCHEMA.to_owned())
        .or_insert_with(connect_error_schema);

    let responses = ensure_nested_object(components, "responses")?;
    responses
        .entry(CONNECT_ERROR_RESPONSE.to_owned())
        .or_insert_with(connect_error_response);

    let Some(paths) = document.get_mut("paths").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    for path_item in paths.values_mut() {
        let Some(path_item) = path_item.as_object_mut() else {
            continue;
        };
        for &method in http_method_names() {
            let Some(operation) = path_item.get_mut(method).and_then(Value::as_object_mut) else {
                continue;
            };
            let responses = operation
                .entry("responses")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .ok_or_else(|| {
                    UniError::from_kind_context(
                        CodegenErrKind::OpenApiInvalidDocument,
                        "operation responses was not an object",
                    )
                })?;
            responses.entry("default".to_owned()).or_insert_with(
                || json!({ "$ref": "#/components/responses/connect2axum.ConnectError" }),
            );
        }
    }

    Ok(())
}

fn connect_error_schema() -> Value {
    json!({
        "type": "object",
        "description": "Connect error response.",
        "required": ["code", "message"],
        "properties": {
            "code": {
                "type": "string",
                "description": "Connect error code."
            },
            "message": {
                "type": "string",
                "description": "Developer-facing error message."
            },
            "details": {
                "type": "array",
                "description": "Optional typed error details.",
                "items": {
                    "type": "object",
                    "additionalProperties": true
                }
            }
        }
    })
}

fn connect_error_response() -> Value {
    json!({
        "description": "Connect error response.",
        "content": {
            "application/json": {
                "schema": {
                    "$ref": "#/components/schemas/connect2axum.ConnectError"
                }
            }
        }
    })
}

fn http_method_names() -> &'static [&'static str] {
    &[
        "get", "put", "post", "delete", "patch", "head", "options", "trace",
    ]
}

fn openapi_method_name(verb: HttpVerb) -> &'static str {
    match verb {
        HttpVerb::Get => "get",
        HttpVerb::Post => "post",
        HttpVerb::Put => "put",
        HttpVerb::Delete => "delete",
        HttpVerb::Patch => "patch",
    }
}
