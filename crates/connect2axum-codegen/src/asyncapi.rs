use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use connectrpc_codegen::plugin::{CodeGeneratorResponse, CodeGeneratorResponseFile};
use serde_json::{Map, Value, json};
use uni_error::UniError;

use crate::CodeGeneratorRequest;
use crate::error::{CodegenErrKind, CodegenResult};
use crate::guardrails::ensure_unique_routes;
use crate::ir::{DescriptorIr, Field, FieldKind, FieldLabel, Method, Service, build_ir};
use crate::openapi::comments::comment_description;
use crate::openapi::config::{DocConfig, InfoConfig};
use crate::openapi::schema::scalar_field_schema;
use crate::options::CodegenOptions;
use crate::shape::{FileShapes, RequestShape, plan_file_shapes};
use crate::ws::ws_route_path;

const DEFAULT_OUTPUT_FILE: &str = "asyncapi.json";
const DEFAULT_CONTENT_TYPE: &str = "application/json";
const ASYNCAPI_VERSION: &str = "3.1.0";

pub(crate) fn generate(request: &CodeGeneratorRequest) -> CodegenResult<CodeGeneratorResponse> {
    let options = AsyncApiOptions::parse(request.parameter.as_deref())?;
    let config = options.load_config()?;
    let ir = build_ir(request)?;
    let Some(document) = build_document(&ir, &options, &config)? else {
        return Ok(CodeGeneratorResponse::default());
    };
    validate_document(&document)?;

    let content = serde_json::to_string_pretty(&document).map_err(|err| {
        UniError::from_kind_context(
            CodegenErrKind::AsyncApiInvalidDocument,
            format!("failed to serialize AsyncAPI document: {err}"),
        )
    })? + "\n";

    Ok(CodeGeneratorResponse {
        file: vec![CodeGeneratorResponseFile {
            name: Some(options.output_file),
            content: Some(content),
            ..Default::default()
        }],
        ..Default::default()
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AsyncApiOptions {
    output_file: String,
    config_path: Option<PathBuf>,
    default_content_type: String,
    server_url: Option<String>,
}

impl Default for AsyncApiOptions {
    fn default() -> Self {
        Self {
            output_file: DEFAULT_OUTPUT_FILE.to_owned(),
            config_path: None,
            default_content_type: DEFAULT_CONTENT_TYPE.to_owned(),
            server_url: None,
        }
    }
}

impl AsyncApiOptions {
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
                "default_content_type" => options.default_content_type = value.to_owned(),
                "server_url" => options.server_url = Some(value.to_owned()),
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
        DocConfig::from_asyncapi_path(path)
    }
}

fn invalid_option(context: String) -> uni_error::UniError<CodegenErrKind> {
    UniError::from_kind_context(CodegenErrKind::InvalidPluginOption, context)
}

fn build_document(
    ir: &DescriptorIr,
    options: &AsyncApiOptions,
    config: &DocConfig,
) -> CodegenResult<Option<Value>> {
    let ws_methods = collect_ws_methods(ir)?;
    if ws_methods.is_empty() {
        return Ok(None);
    }

    let mut schema_registry = SchemaRegistry::default();
    let mut channels = Map::new();
    let mut operations = Map::new();
    let mut messages = Map::new();
    let mut tags = BTreeMap::new();

    for ws_method in &ws_methods {
        schema_registry.ensure_schema(ir, ws_method.method.input_type.as_ref());
        schema_registry.ensure_schema(ir, ws_method.method.output_type.as_ref());

        let service_tag = service_tag(ws_method.service);
        tags.entry(ws_method.service.name.as_ref().to_owned())
            .or_insert_with(|| service_tag.clone());

        let request_message_key = message_component_key(ws_method.method, "request");
        let response_message_key = message_component_key(ws_method.method, "response");

        messages.insert(
            request_message_key.clone(),
            message_component(
                ir,
                ws_method.method.input_type.as_ref(),
                &request_message_key,
                &options.default_content_type,
            ),
        );
        messages.insert(
            response_message_key.clone(),
            message_component(
                ir,
                ws_method.method.output_type.as_ref(),
                &response_message_key,
                &options.default_content_type,
            ),
        );

        channels.insert(
            ws_method.route_path.clone(),
            channel_object(
                ws_method,
                &request_message_key,
                &response_message_key,
                config,
            ),
        );

        operations.insert(
            operation_key(ws_method.method, "receive"),
            operation_object(
                ws_method,
                "receive",
                "request",
                ws_method.request_cardinality(),
                config,
            ),
        );
        operations.insert(
            operation_key(ws_method.method, "send"),
            operation_object(
                ws_method,
                "send",
                "response",
                ws_method.response_cardinality(),
                config,
            ),
        );
    }

    let mut components = Map::new();
    if !schema_registry.schemas.is_empty() {
        components.insert("schemas".to_owned(), Value::Object(schema_registry.schemas));
    }
    components.insert("messages".to_owned(), Value::Object(messages));
    if !config.security_schemes.is_empty() {
        components.insert(
            "securitySchemes".to_owned(),
            Value::Object(
                config
                    .security_schemes
                    .iter()
                    .map(|(name, scheme)| (name.clone(), scheme.clone()))
                    .collect(),
            ),
        );
    }

    let mut document = Map::new();
    document.insert(
        "asyncapi".to_owned(),
        Value::String(ASYNCAPI_VERSION.to_owned()),
    );
    document.insert("info".to_owned(), info_object(&config.info));
    document.insert(
        "defaultContentType".to_owned(),
        Value::String(options.default_content_type.clone()),
    );

    let servers = servers_object(config, options)?;
    if !servers.is_empty() {
        document.insert("servers".to_owned(), Value::Object(servers));
    }
    document.insert("channels".to_owned(), Value::Object(channels));
    document.insert("operations".to_owned(), Value::Object(operations));
    document.insert("components".to_owned(), Value::Object(components));
    if !tags.is_empty() {
        document.insert(
            "tags".to_owned(),
            Value::Array(tags.into_values().collect::<Vec<_>>()),
        );
    }

    Ok(Some(Value::Object(document)))
}

fn collect_ws_methods(ir: &DescriptorIr) -> CodegenResult<Vec<AsyncWsMethod<'_>>> {
    let mut methods = Vec::new();

    for file_name in &ir.files_to_generate {
        let Some(file) = ir.file(file_name.as_ref()) else {
            return Err(UniError::from_kind_context(
                CodegenErrKind::FileToGenerateNotFound,
                format!("file_to_generate {file_name:?} was not present in proto_file"),
            ));
        };
        if !file.has_http_bindings() {
            continue;
        }

        let shapes = plan_file_shapes(ir, file, &CodegenOptions::default())?;
        for service in &file.services {
            for method in &service.methods {
                let Some(binding) = method.http.as_ref() else {
                    continue;
                };
                let Some(kind) = AsyncWsMethodKind::from_method(method) else {
                    continue;
                };
                let shape = shape_for(&shapes, method)?;
                if matches!(kind, AsyncWsMethodKind::Server) && has_path_or_query(shape) {
                    eprintln!(
                        "warning: connect2asyncapi skipping {}.{} at {} because connect2ws does not generate WebSocket routes for server-streaming methods with path/query bindings",
                        service.full_name.as_ref(),
                        method.name.as_ref(),
                        binding.path.as_ref()
                    );
                    continue;
                }

                methods.push(AsyncWsMethod {
                    service,
                    method,
                    kind,
                    route_path: ws_route_path(binding.path.as_ref()),
                });
            }
        }
    }

    ensure_unique_routes(
        "AsyncAPI WebSocket routes",
        methods.iter().map(|method| {
            (
                method.route_path.clone(),
                format!("method {}", method.method.full_name.as_ref()),
            )
        }),
    )?;

    Ok(methods)
}

fn shape_for<'a>(shapes: &'a FileShapes, method: &Method) -> CodegenResult<&'a RequestShape> {
    shapes
        .request_shapes
        .iter()
        .find(|shape| shape.method == method.full_name)
        .ok_or_else(|| {
            UniError::from_kind_context(
                CodegenErrKind::InvalidDescriptor,
                format!(
                    "request shape for {} was not planned",
                    method.full_name.as_ref()
                ),
            )
        })
}

fn has_path_or_query(shape: &RequestShape) -> bool {
    !shape.path_fields.is_empty() || shape.query_shape.is_some()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AsyncWsMethodKind {
    Server,
    Client,
    Bidi,
}

impl AsyncWsMethodKind {
    fn from_method(method: &Method) -> Option<Self> {
        match (method.client_streaming, method.server_streaming) {
            (false, false) => None,
            (false, true) => Some(Self::Server),
            (true, false) => Some(Self::Client),
            (true, true) => Some(Self::Bidi),
        }
    }
}

struct AsyncWsMethod<'a> {
    service: &'a Service,
    method: &'a Method,
    kind: AsyncWsMethodKind,
    route_path: String,
}

impl AsyncWsMethod<'_> {
    fn request_cardinality(&self) -> &'static str {
        match self.kind {
            AsyncWsMethodKind::Server => "single",
            AsyncWsMethodKind::Client | AsyncWsMethodKind::Bidi => "stream",
        }
    }

    fn response_cardinality(&self) -> &'static str {
        match self.kind {
            AsyncWsMethodKind::Client => "single",
            AsyncWsMethodKind::Server | AsyncWsMethodKind::Bidi => "stream",
        }
    }
}

fn info_object(info: &InfoConfig) -> Value {
    let mut value = Map::new();
    value.insert(
        "title".to_owned(),
        Value::String(info.title.clone().unwrap_or_else(|| "API".to_owned())),
    );
    value.insert(
        "version".to_owned(),
        Value::String(info.version.clone().unwrap_or_else(|| "1.0.0".to_owned())),
    );
    if let Some(description) = info.description.as_ref() {
        value.insert(
            "description".to_owned(),
            Value::String(description.to_owned()),
        );
    }
    if let Some(terms) = info.terms_of_service.as_ref() {
        value.insert("termsOfService".to_owned(), Value::String(terms.to_owned()));
    }
    Value::Object(value)
}

fn servers_object(
    config: &DocConfig,
    options: &AsyncApiOptions,
) -> CodegenResult<Map<String, Value>> {
    let mut servers = Map::new();
    if let Some(url) = options.server_url.as_ref() {
        servers.insert("default".to_owned(), server_from_url(url, None)?);
    }

    for (index, server) in config.servers.iter().enumerate() {
        let name = if index == 0 && servers.is_empty() {
            "default".to_owned()
        } else {
            format!("server{index}")
        };
        servers.insert(name, normalize_server(server)?);
    }

    Ok(servers)
}

fn normalize_server(server: &Value) -> CodegenResult<Value> {
    let Some(object) = server.as_object() else {
        return Err(UniError::from_kind_context(
            CodegenErrKind::AsyncApiInvalidDocument,
            "AsyncAPI server config entries must be objects",
        ));
    };
    if object.contains_key("host") && object.contains_key("protocol") {
        return Ok(server.clone());
    }

    let Some(url) = object.get("url").and_then(Value::as_str) else {
        return Err(UniError::from_kind_context(
            CodegenErrKind::AsyncApiInvalidDocument,
            "AsyncAPI server config entries must contain host/protocol or url",
        ));
    };
    server_from_url(
        url,
        object
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
    )
}

fn server_from_url(url: &str, description: Option<String>) -> CodegenResult<Value> {
    let (protocol, rest) = url.split_once("://").ok_or_else(|| {
        UniError::from_kind_context(
            CodegenErrKind::AsyncApiInvalidDocument,
            format!("AsyncAPI server_url {url:?} must include a URL scheme"),
        )
    })?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if host.is_empty() {
        return Err(UniError::from_kind_context(
            CodegenErrKind::AsyncApiInvalidDocument,
            format!("AsyncAPI server_url {url:?} must include a host"),
        ));
    }
    let protocol = match protocol {
        "ws" | "wss" => protocol,
        "http" => "ws",
        "https" => "wss",
        value => value,
    };

    let mut server = Map::new();
    server.insert("host".to_owned(), Value::String(host.to_owned()));
    server.insert("protocol".to_owned(), Value::String(protocol.to_owned()));
    if !path.is_empty() {
        server.insert("pathname".to_owned(), Value::String(format!("/{path}")));
    }
    if let Some(description) = description {
        server.insert("description".to_owned(), Value::String(description));
    }
    Ok(Value::Object(server))
}

fn channel_object(
    ws_method: &AsyncWsMethod<'_>,
    request_message_key: &str,
    response_message_key: &str,
    config: &DocConfig,
) -> Value {
    let mut channel_messages = Map::new();
    channel_messages.insert(
        "request".to_owned(),
        component_message_ref(request_message_key),
    );
    channel_messages.insert(
        "response".to_owned(),
        component_message_ref(response_message_key),
    );

    let mut channel = Map::new();
    channel.insert(
        "address".to_owned(),
        Value::String(ws_method.route_path.clone()),
    );
    channel.insert("messages".to_owned(), Value::Object(channel_messages));
    if let Some(security) = asyncapi_security(config) {
        channel.insert("security".to_owned(), security);
    }
    channel.insert(
        "x-connect2axum-websocket".to_owned(),
        json!({
            "json": true,
            "route": ws_method.route_path,
            "rpc": ws_method.method.full_name.as_ref(),
        }),
    );
    Value::Object(channel)
}

fn operation_object(
    ws_method: &AsyncWsMethod<'_>,
    action: &str,
    channel_message_key: &str,
    cardinality: &str,
    config: &DocConfig,
) -> Value {
    let mut operation = Map::new();
    operation.insert("action".to_owned(), Value::String(action.to_owned()));
    operation.insert("channel".to_owned(), channel_ref(&ws_method.route_path));
    operation.insert(
        "messages".to_owned(),
        Value::Array(vec![channel_message_ref(
            &ws_method.route_path,
            channel_message_key,
        )]),
    );
    operation.insert(
        "summary".to_owned(),
        Value::String(operation_summary(ws_method, action)),
    );
    if let Some(description) = comment_description(&ws_method.method.comments) {
        operation.insert("description".to_owned(), Value::String(description));
    }
    operation.insert(
        "tags".to_owned(),
        Value::Array(vec![service_tag(ws_method.service)]),
    );
    if let Some(security) = asyncapi_security(config) {
        operation.insert("security".to_owned(), security);
    }
    operation.insert(
        "x-connect2axum-streaming".to_owned(),
        json!({
            "transport": "websocket",
            "framing": "json-text-frame",
            "direction": action,
            "cardinality": cardinality,
            "rpc": ws_method.method.full_name.as_ref(),
        }),
    );
    if action == "receive" && ws_method.kind != AsyncWsMethodKind::Server {
        operation.insert(
            "x-connect2axum-end-of-stream".to_owned(),
            json!({
                "frameType": "text",
                "payload": "",
                "description": "An empty text frame ends the client request stream while keeping the WebSocket open for response frames."
            }),
        );
    }
    Value::Object(operation)
}

fn operation_summary(ws_method: &AsyncWsMethod<'_>, action: &str) -> String {
    let direction = if action == "receive" {
        "Receive"
    } else {
        "Send"
    };
    format!(
        "{direction} {} {}",
        ws_method.service.name.as_ref(),
        ws_method.method.name.as_ref()
    )
}

fn service_tag(service: &Service) -> Value {
    let mut tag = Map::new();
    tag.insert(
        "name".to_owned(),
        Value::String(service.name.as_ref().to_owned()),
    );
    if let Some(description) = comment_description(&service.comments) {
        tag.insert("description".to_owned(), Value::String(description));
    }
    Value::Object(tag)
}

fn message_component(
    ir: &DescriptorIr,
    message_type: &str,
    name: &str,
    default_content_type: &str,
) -> Value {
    let mut message = Map::new();
    message.insert("name".to_owned(), Value::String(name.to_owned()));
    message.insert(
        "contentType".to_owned(),
        Value::String(default_content_type.to_owned()),
    );
    if let Some(proto_message) = ir.message(message_type)
        && let Some(description) = comment_description(&proto_message.comments)
    {
        message.insert("description".to_owned(), Value::String(description));
    }
    message.insert("payload".to_owned(), schema_ref(message_type));
    Value::Object(message)
}

#[derive(Default)]
struct SchemaRegistry {
    schemas: Map<String, Value>,
    seen: BTreeSet<String>,
}

impl SchemaRegistry {
    fn ensure_schema(&mut self, ir: &DescriptorIr, full_name: &str) {
        if !self.seen.insert(full_name.to_owned()) {
            return;
        }

        let schema = if full_name == "google.protobuf.Empty" {
            json!({ "type": "object", "properties": {} })
        } else if let Some(message) = ir.message(full_name) {
            self.message_schema(ir, message)
        } else {
            json!({ "type": "object", "additionalProperties": true })
        };
        self.schemas.insert(full_name.to_owned(), schema);
    }

    fn message_schema(&mut self, ir: &DescriptorIr, message: &crate::ir::Message) -> Value {
        let mut schema = Map::new();
        schema.insert("type".to_owned(), Value::String("object".to_owned()));
        if let Some(description) = comment_description(&message.comments) {
            schema.insert("description".to_owned(), Value::String(description));
        }

        let properties = message
            .fields
            .iter()
            .map(|field| {
                (
                    field.json_name.as_ref().to_owned(),
                    self.field_schema(ir, field),
                )
            })
            .collect::<Map<_, _>>();
        schema.insert("properties".to_owned(), Value::Object(properties));
        Value::Object(schema)
    }

    fn field_schema(&mut self, ir: &DescriptorIr, field: &Field) -> Value {
        let mut schema = if field.label == Some(FieldLabel::Repeated) {
            json!({
                "type": "array",
                "items": self.single_field_schema(ir, &field.kind)
            })
        } else {
            self.single_field_schema(ir, &field.kind)
        };

        if let Some(description) = comment_description(&field.comments)
            && let Some(schema) = schema.as_object_mut()
        {
            schema.insert("description".to_owned(), Value::String(description));
        }

        schema
    }

    fn single_field_schema(&mut self, ir: &DescriptorIr, kind: &FieldKind) -> Value {
        match kind {
            FieldKind::Message(full_name) | FieldKind::Group(full_name) => {
                self.ensure_schema(ir, full_name.as_ref());
                schema_ref(full_name.as_ref())
            }
            _ => scalar_field_schema(kind),
        }
    }
}

fn asyncapi_security(config: &DocConfig) -> Option<Value> {
    let security = config.security.as_ref()?;
    let Some(requirements) = security.as_array() else {
        return Some(security.clone());
    };

    let schemes = requirements
        .iter()
        .filter_map(Value::as_object)
        .flat_map(|requirement| requirement.keys())
        .filter(|name| config.security_schemes.contains_key(*name))
        .map(|name| json!({ "$ref": format!("#/components/securitySchemes/{}", json_pointer_escape(name)) }))
        .collect::<Vec<_>>();

    if schemes.is_empty() {
        Some(security.clone())
    } else {
        Some(Value::Array(schemes))
    }
}

fn operation_key(method: &Method, action: &str) -> String {
    format!("{}_{}", component_name(method.full_name.as_ref()), action)
}

fn message_component_key(method: &Method, direction: &str) -> String {
    format!("{}.{}", method.full_name.as_ref(), direction)
}

fn component_name(value: &str) -> String {
    let mut name = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if name.starts_with('_') {
        name.remove(0);
    }
    name
}

fn schema_ref(full_name: &str) -> Value {
    json!({ "$ref": format!("#/components/schemas/{}", json_pointer_escape(full_name)) })
}

fn component_message_ref(name: &str) -> Value {
    json!({ "$ref": format!("#/components/messages/{}", json_pointer_escape(name)) })
}

fn channel_ref(route_path: &str) -> Value {
    json!({ "$ref": format!("#/channels/{}", json_pointer_escape(route_path)) })
}

fn channel_message_ref(route_path: &str, message: &str) -> Value {
    json!({ "$ref": format!("#/channels/{}/messages/{message}", json_pointer_escape(route_path)) })
}

fn json_pointer_escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn validate_document(document: &Value) -> CodegenResult<()> {
    let Some(object) = document.as_object() else {
        return Err(UniError::from_kind_context(
            CodegenErrKind::AsyncApiInvalidDocument,
            "AsyncAPI document root was not an object",
        ));
    };
    for field in ["asyncapi", "info", "channels", "operations", "components"] {
        if !object.contains_key(field) {
            return Err(UniError::from_kind_context(
                CodegenErrKind::AsyncApiInvalidDocument,
                format!("AsyncAPI document was missing required field {field}"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use connectrpc_codegen::codegen::descriptor::{
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto,
        field_descriptor_proto::{Label, Type},
    };
    use flexstr::ToOwnedFlexStr as _;
    use serde_json::json;

    use super::{AsyncApiOptions, build_document};
    use crate::ir::{
        CommentSet, DescriptorIr, Field, FieldKind, FieldLabel, HttpBinding, HttpBody, HttpVerb,
        Message, Method, ProtoFile, Service,
    };
    use crate::openapi::config::{DocConfig, InfoConfig};

    #[test]
    fn parses_asyncapi_options() {
        let options = AsyncApiOptions::parse(Some(
            "output=docs/asyncapi.json,config=asyncapi.yaml,default_content_type=application/custom+json,server_url=wss://api.example.test/ws",
        ))
        .unwrap();

        assert_eq!(options.output_file, "docs/asyncapi.json");
        assert_eq!(
            options
                .config_path
                .as_deref()
                .and_then(|path| path.to_str()),
            Some("asyncapi.yaml")
        );
        assert_eq!(options.default_content_type, "application/custom+json");
        assert_eq!(
            options.server_url.as_deref(),
            Some("wss://api.example.test/ws")
        );
    }

    #[test]
    fn documents_streaming_websocket_routes() {
        let ir = test_ir();
        let document =
            build_document(&ir, &AsyncApiOptions::default(), &DocConfig::default()).unwrap();
        let document = document.unwrap();

        assert!(document["channels"].get("/hello/expand/ws").is_some());
        assert!(document["channels"].get("/hello/collect/ws").is_some());
        assert!(document["channels"].get("/hello/chat/ws").is_some());
        assert!(document["channels"].get("/hello/unary/ws").is_none());

        assert!(
            document["operations"]
                .get("streaming_v1_GreeterService_Expand_receive")
                .is_some()
        );
        assert_eq!(
            document["operations"]["streaming_v1_GreeterService_Collect_receive"]["x-connect2axum-end-of-stream"]
                ["payload"],
            ""
        );
        assert_eq!(
            document["components"]["schemas"]["streaming.v1.HelloSummary"]["properties"]["names"]["type"],
            "array"
        );
    }

    #[test]
    fn applies_config_security_and_servers() {
        let ir = test_ir();
        let mut security_schemes = std::collections::BTreeMap::new();
        security_schemes.insert(
            "BearerAuth".to_owned(),
            json!({ "type": "http", "scheme": "bearer", "bearerFormat": "JWT" }),
        );
        let config = DocConfig {
            info: InfoConfig {
                title: Some("Streaming API".to_owned()),
                version: Some("0.2.0".to_owned()),
                description: Some("Generated websocket docs.".to_owned()),
                ..Default::default()
            },
            servers: vec![json!({
                "url": "ws://127.0.0.1:8002",
                "description": "Local WebSocket server"
            })],
            security_schemes,
            security: Some(json!([{ "BearerAuth": [] }])),
            ..Default::default()
        };

        let document = build_document(&ir, &AsyncApiOptions::default(), &config)
            .unwrap()
            .unwrap();

        assert_eq!(document["info"]["title"], "Streaming API");
        assert_eq!(document["servers"]["default"]["protocol"], "ws");
        assert_eq!(document["servers"]["default"]["host"], "127.0.0.1:8002");
        assert_eq!(
            document["components"]["securitySchemes"]["BearerAuth"]["scheme"],
            "bearer"
        );
        assert_eq!(
            document["operations"]["streaming_v1_GreeterService_Expand_receive"]["security"][0]["$ref"],
            "#/components/securitySchemes/BearerAuth"
        );
    }

    #[test]
    fn skips_server_streaming_routes_with_path_or_query_bindings() {
        let mut ir = test_ir();
        ir.files[0].services[0].methods[0].http = Some(HttpBinding {
            verb: HttpVerb::Post,
            path: "/hello/{first_name}/expand".into(),
            body: HttpBody::Wildcard,
            path_variables: vec!["first_name".into()],
        });

        let document =
            build_document(&ir, &AsyncApiOptions::default(), &DocConfig::default()).unwrap();
        let document = document.unwrap();

        assert!(document["channels"].get("/hello/expand/ws").is_none());
        assert!(document["channels"].get("/hello/chat/ws").is_some());
    }

    fn test_ir() -> DescriptorIr {
        DescriptorIr {
            files_to_generate: vec!["streaming.proto".into()],
            descriptor_files: vec![descriptor_file()],
            files: vec![ProtoFile {
                name: "streaming.proto".into(),
                package: "streaming.v1".into(),
                messages: vec![
                    Message {
                        name: "HelloRequest".into(),
                        full_name: "streaming.v1.HelloRequest".into(),
                        comments: comments("Request payload."),
                        fields: vec![
                            string_field("first_name", "firstName"),
                            string_field("last_name", "lastName"),
                        ],
                        messages: Vec::new(),
                    },
                    Message {
                        name: "HelloReply".into(),
                        full_name: "streaming.v1.HelloReply".into(),
                        comments: comments("Response payload."),
                        fields: vec![string_field("message", "message")],
                        messages: Vec::new(),
                    },
                    Message {
                        name: "HelloSummary".into(),
                        full_name: "streaming.v1.HelloSummary".into(),
                        comments: CommentSet::default(),
                        fields: vec![Field {
                            name: "names".into(),
                            json_name: "names".into(),
                            number: Some(1),
                            label: Some(FieldLabel::Repeated),
                            kind: FieldKind::String,
                            comments: CommentSet::default(),
                        }],
                        messages: Vec::new(),
                    },
                ],
                services: vec![Service {
                    name: "GreeterService".into(),
                    full_name: "streaming.v1.GreeterService".into(),
                    comments: comments("Greeter websocket service."),
                    methods: vec![
                        method("Expand", false, true, "/hello/expand", "HelloReply"),
                        method("Collect", true, false, "/hello/collect", "HelloSummary"),
                        method("Chat", true, true, "/hello/chat", "HelloReply"),
                        method("Unary", false, false, "/hello/unary", "HelloReply"),
                    ],
                }],
            }],
        }
    }

    fn descriptor_file() -> FileDescriptorProto {
        FileDescriptorProto {
            name: Some("streaming.proto".to_owned()),
            package: Some("streaming.v1".to_owned()),
            message_type: vec![
                descriptor_message(
                    "HelloRequest",
                    vec![
                        descriptor_string_field(
                            "first_name",
                            "firstName",
                            1,
                            Label::LABEL_OPTIONAL,
                        ),
                        descriptor_string_field("last_name", "lastName", 2, Label::LABEL_OPTIONAL),
                    ],
                ),
                descriptor_message(
                    "HelloReply",
                    vec![descriptor_string_field(
                        "message",
                        "message",
                        1,
                        Label::LABEL_OPTIONAL,
                    )],
                ),
                descriptor_message(
                    "HelloSummary",
                    vec![descriptor_string_field(
                        "names",
                        "names",
                        1,
                        Label::LABEL_REPEATED,
                    )],
                ),
            ],
            ..Default::default()
        }
    }

    fn descriptor_message(name: &str, fields: Vec<FieldDescriptorProto>) -> DescriptorProto {
        DescriptorProto {
            name: Some(name.to_owned()),
            field: fields,
            ..Default::default()
        }
    }

    fn descriptor_string_field(
        name: &str,
        json_name: &str,
        number: i32,
        label: Label,
    ) -> FieldDescriptorProto {
        FieldDescriptorProto {
            name: Some(name.to_owned()),
            json_name: Some(json_name.to_owned()),
            number: Some(number),
            label: Some(label),
            r#type: Some(Type::TYPE_STRING),
            ..Default::default()
        }
    }

    fn method(
        name: &str,
        client_streaming: bool,
        server_streaming: bool,
        path: &str,
        output: &str,
    ) -> Method {
        Method {
            name: name.to_owned_opt(),
            full_name: format!("streaming.v1.GreeterService.{name}").into(),
            input_type: "streaming.v1.HelloRequest".into(),
            output_type: format!("streaming.v1.{output}").into(),
            client_streaming,
            server_streaming,
            comments: comments(&format!("{name} method.")),
            http: Some(HttpBinding {
                verb: HttpVerb::Post,
                path: path.to_owned_opt(),
                body: HttpBody::Wildcard,
                path_variables: Vec::new(),
            }),
        }
    }

    fn string_field(name: &str, json_name: &str) -> Field {
        Field {
            name: name.to_owned_opt(),
            json_name: json_name.to_owned_opt(),
            number: Some(1),
            label: Some(FieldLabel::Optional),
            kind: FieldKind::String,
            comments: CommentSet::default(),
        }
    }

    fn comments(value: &str) -> CommentSet {
        CommentSet {
            leading_detached: Vec::new(),
            leading: Some(value.to_owned_opt()),
            trailing: None,
        }
    }
}
