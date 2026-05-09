use std::collections::BTreeMap;
use std::env;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use buffa::Message as _;
use connectrpc_codegen::codegen::descriptor::FileDescriptorProto;
use connectrpc_codegen::plugin::{CodeGeneratorResponse, CodeGeneratorResponseFile};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use uni_error::UniError;

use crate::CodeGeneratorRequest;
use crate::error::{CodegenErrKind, CodegenResult};
use crate::ir::{CommentSet, DescriptorIr, FieldKind, FieldLabel, HttpVerb, ProtoFile, build_ir};
use crate::options::CodegenOptions;
use crate::shape::{RequestPartShape, ShapeField, plan_file_shapes};

const DEFAULT_OUTPUT_FILE: &str = "openapi.json";
const DEFAULT_STREAMING_CONTENT_TYPE: &str = "application/x-ndjson";
const DEFAULT_OPENAPIV3_BIN: &str = "protoc-gen-openapiv3";
const OPENAPI_JSON_CONTENT_TYPE: &str = "application/json";
const CONNECT_ERROR_SCHEMA: &str = "connect2axum.ConnectError";
const CONNECT_ERROR_RESPONSE: &str = "connect2axum.ConnectError";

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
    child_request.parameter = Some(options.openapiv3_parameter());

    let child = run_openapiv3(&options.openapiv3_binary()?, &child_request)?;
    let supported_features = child.supported_features;
    let minimum_edition = child.minimum_edition;
    let maximum_edition = child.maximum_edition;
    let document = merge_openapi_documents(child.file, &ir, &options, &config)?;
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

    fn load_config(&self) -> CodegenResult<OpenApiConfig> {
        let Some(path) = self.config_path.as_ref() else {
            return Ok(OpenApiConfig::default());
        };
        OpenApiConfig::from_path(path)
    }

    fn openapiv3_binary(&self) -> CodegenResult<PathBuf> {
        if let Some(path) = self.openapiv3_bin.as_ref() {
            return Ok(path.clone());
        }
        if let Ok(path) = env::var("CONNECT2AXUM_OPENAPIV3_BIN")
            && !path.trim().is_empty()
        {
            return Ok(PathBuf::from(path));
        }
        if command_exists(DEFAULT_OPENAPIV3_BIN) {
            return Ok(PathBuf::from(DEFAULT_OPENAPIV3_BIN));
        }
        if let Some(home) = env::var_os("HOME") {
            let path = PathBuf::from(home)
                .join("go")
                .join("bin")
                .join(DEFAULT_OPENAPIV3_BIN);
            if path.is_file() {
                return Ok(path);
            }
        }

        Err(UniError::from_kind_context(
            CodegenErrKind::OpenApiPluginFailed,
            "could not find protoc-gen-openapiv3; set openapiv3_bin=... or CONNECT2AXUM_OPENAPIV3_BIN",
        ))
    }

    fn openapiv3_parameter(&self) -> String {
        let mut options = self.openapiv3_options.clone();
        if !options
            .iter()
            .any(|option| option.starts_with("disable_default_errors="))
        {
            options.insert(0, "disable_default_errors=true".to_owned());
        }
        options.join(",")
    }
}

fn invalid_option(context: String) -> uni_error::UniError<CodegenErrKind> {
    UniError::from_kind_context(CodegenErrKind::InvalidPluginOption, context)
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenApiConfig {
    info: InfoConfig,
    servers: Vec<Value>,
    security_schemes: BTreeMap<String, Value>,
    security: Option<Value>,
    headers: Vec<HeaderConfig>,
    streaming_content_type: Option<String>,
    default_error_response: Option<bool>,
}

impl OpenApiConfig {
    fn from_path(path: &Path) -> CodegenResult<Self> {
        let content = std::fs::read_to_string(path).map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiInvalidDocument,
                format!("failed to read OpenAPI config {}: {err}", path.display()),
            )
        })?;

        serde_yaml::from_str(&content).map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiInvalidDocument,
                format!("failed to parse OpenAPI config {}: {err}", path.display()),
            )
        })
    }

    fn streaming_content_type<'a>(&'a self, options: &'a OpenApiOptions) -> &'a str {
        self.streaming_content_type
            .as_deref()
            .unwrap_or(&options.streaming_content_type)
    }

    fn add_default_error_response(&self) -> bool {
        self.default_error_response.unwrap_or(true)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct InfoConfig {
    title: Option<String>,
    version: Option<String>,
    description: Option<String>,
    terms_of_service: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct HeaderConfig {
    name: String,
    required: bool,
    description: Option<String>,
    schema: Value,
}

impl Default for HeaderConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            required: false,
            description: None,
            schema: json!({ "type": "string" }),
        }
    }
}

fn run_openapiv3(
    binary: &Path,
    request: &CodeGeneratorRequest,
) -> CodegenResult<CodeGeneratorResponse> {
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiPluginFailed,
                format!("failed to start {}: {err}", binary.display()),
            )
        })?;

    let request_bytes = request.encode_to_vec();
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(&request_bytes)
        .map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiPluginFailed,
                format!(
                    "failed to write CodeGeneratorRequest to {}: {err}",
                    binary.display()
                ),
            )
        })?;

    let output = child.wait_with_output().map_err(|err| {
        UniError::from_kind_context(
            CodegenErrKind::OpenApiPluginFailed,
            format!("failed to wait for {}: {err}", binary.display()),
        )
    })?;

    let response = if output.stdout.is_empty() {
        CodeGeneratorResponse::default()
    } else {
        CodeGeneratorResponse::decode_from_slice(&output.stdout).map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiPluginFailed,
                format!("failed to decode protoc-gen-openapiv3 response: {err}"),
            )
        })?
    };

    if let Some(error) = response.error.as_ref() {
        return Err(UniError::from_kind_context(
            CodegenErrKind::OpenApiPluginFailed,
            format!("protoc-gen-openapiv3 failed: {error}"),
        ));
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(UniError::from_kind_context(
            CodegenErrKind::OpenApiPluginFailed,
            format!(
                "protoc-gen-openapiv3 exited with status {}{}",
                output.status,
                if stderr.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", stderr.trim())
                }
            ),
        ));
    }

    Ok(response)
}

fn command_exists(command: &str) -> bool {
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|path| {
            let candidate = path.join(command);
            candidate.is_file()
        })
    })
}

fn inject_go_packages(request: &mut CodeGeneratorRequest) {
    for file in request
        .proto_file
        .iter_mut()
        .chain(request.source_file_descriptors.iter_mut())
    {
        let options = file.options.get_or_insert_default();
        if options
            .go_package
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            continue;
        }

        let go_package = synthetic_go_package(file);
        file.options.get_or_insert_default().go_package = Some(go_package);
    }
}

fn synthetic_go_package(file: &FileDescriptorProto) -> String {
    let name = file.name.as_deref().unwrap_or("schema.proto");
    let stem = name.strip_suffix(".proto").unwrap_or(name);
    let import_path = format!("connect2axum.local/gen/{}", sanitize_go_import_path(stem));
    let alias_source = file
        .package
        .as_deref()
        .and_then(|package| package.rsplit('.').next())
        .filter(|part| !part.is_empty())
        .unwrap_or(stem);
    let alias = sanitize_go_package_alias(alias_source);
    format!("{import_path};{alias}")
}

fn sanitize_go_import_path(value: &str) -> String {
    value
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn sanitize_go_package_alias(value: &str) -> String {
    let mut alias = value
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                Some(ch.to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect::<String>();

    if alias.is_empty() {
        alias.push_str("schema");
    }
    if alias.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        alias.insert(0, 'p');
    }
    alias
}

fn merge_openapi_documents(
    files: Vec<CodeGeneratorResponseFile>,
    ir: &DescriptorIr,
    options: &OpenApiOptions,
    config: &OpenApiConfig,
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
        let name = file.name.as_deref().unwrap_or(DEFAULT_OUTPUT_FILE);
        let parse_name = if name.is_empty() {
            DEFAULT_OUTPUT_FILE
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
    patch_streaming_operations(&mut merged, ir, config.streaming_content_type(options))?;
    if config.add_default_error_response() {
        add_connect_error_response(&mut merged)?;
    }

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

fn merge_document(target: &mut Value, source: Value, source_name: &str) -> CodegenResult<()> {
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

fn merge_named_values(
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

fn apply_config(document: &mut Value, config: &OpenApiConfig) -> CodegenResult<()> {
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

fn generated_dto_schema(fields: &[ShapeField]) -> Value {
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

fn field_schema(field: &ShapeField) -> Value {
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

fn scalar_field_schema(kind: &FieldKind) -> Value {
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

fn comment_description(comments: &CommentSet) -> Option<String> {
    let lines = comments
        .leading_detached
        .iter()
        .chain(comments.leading.iter())
        .flat_map(|comment| comment.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn patch_streaming_operations(
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

fn add_connect_error_response(document: &mut Value) -> CodegenResult<()> {
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

fn ensure_object_at<'a>(root: &'a mut Value, field: &str) -> &'a mut Map<String, Value> {
    root.as_object_mut()
        .expect("OpenAPI document root should be an object")
        .entry(field.to_owned())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("OpenAPI root field should be an object")
}

fn ensure_nested_object<'a>(
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

fn ensure_array_at<'a>(root: &'a mut Value, field: &str) -> CodegenResult<&'a mut Vec<Value>> {
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

#[cfg(test)]
mod tests {
    use buffa::MessageField;
    use connectrpc_codegen::codegen::descriptor::{
        DescriptorProto, FileDescriptorProto, MethodDescriptorProto, ServiceDescriptorProto,
        field_descriptor_proto::{Label, Type},
    };
    use serde_json::json;

    use super::{
        HeaderConfig, InfoConfig, OpenApiConfig, OpenApiOptions, add_connect_error_response,
        apply_config, inject_go_packages, merge_document, merge_openapi_documents,
        patch_streaming_operations,
    };
    use crate::ir::{
        DescriptorIr, Field, FieldKind, FieldLabel, HttpBinding, HttpBody, HttpVerb, Message,
        Method, ProtoFile, Service,
    };

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
        let config = OpenApiConfig {
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
    fn merges_multiple_child_documents() {
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
            &OpenApiOptions::default(),
            &OpenApiConfig::default(),
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
