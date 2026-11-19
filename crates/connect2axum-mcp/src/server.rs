use std::collections::HashMap;
use std::sync::Arc;

use flexstr::SharedStr;
use futures_util::future::{Either, select};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, JsonObject,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use uni_error::UniError;

use crate::error::{McpErrKind, McpResult};
use crate::tool::{BoxHandler, ToolResult, Tools};

const DEFAULT_PATH: &str = "/mcp";
const MAX_TOOL_NAME_LEN: usize = 128;

/// Server identity and HTTP settings for [`make_router`].
#[derive(Clone, Debug)]
#[must_use]
pub struct McpConfig {
    name: SharedStr,
    version: SharedStr,
    title: Option<SharedStr>,
    instructions: Option<SharedStr>,
    path: SharedStr,
    allowed_hosts: Option<Vec<SharedStr>>,
    allow_any_host: bool,
    allowed_origins: Vec<SharedStr>,
    max_request_body_bytes: Option<usize>,
}

impl McpConfig {
    /// Creates a configuration with the server name and version reported to
    /// clients.
    ///
    /// By default the endpoint is served at `/mcp` and only accepts requests
    /// whose `Host` is `localhost`, `127.0.0.1` or `::1`, which protects local
    /// servers from DNS rebinding. A deployed server must call
    /// [`allowed_hosts`](Self::allowed_hosts) or
    /// [`allow_any_host`](Self::allow_any_host).
    pub fn new(name: impl Into<SharedStr>, version: impl Into<SharedStr>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            title: None,
            instructions: None,
            path: DEFAULT_PATH.into(),
            allowed_hosts: None,
            allow_any_host: false,
            allowed_origins: Vec::new(),
            max_request_body_bytes: None,
        }
    }

    /// A human-readable server title.
    pub fn title(mut self, title: impl Into<SharedStr>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Instructions sent to clients describing how to use the server.
    pub fn instructions(mut self, instructions: impl Into<SharedStr>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// The route path of the MCP endpoint. Defaults to `/mcp`.
    pub fn path(mut self, path: impl Into<SharedStr>) -> Self {
        self.path = path.into();
        self
    }

    /// Hostnames or `host:port` authorities accepted in the `Host` header.
    /// Replaces the loopback default.
    pub fn allowed_hosts<I>(mut self, hosts: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<SharedStr>,
    {
        self.allowed_hosts = Some(hosts.into_iter().map(Into::into).collect());
        self.allow_any_host = false;
        self
    }

    /// Accepts any `Host` header. Use only when another layer, such as a
    /// reverse proxy, already restricts hosts.
    pub fn allow_any_host(mut self) -> Self {
        self.allow_any_host = true;
        self.allowed_hosts = None;
        self
    }

    /// Browser origins accepted in the `Origin` header, such as
    /// `https://app.example.com:443`. Empty, the default, disables the check.
    pub fn allowed_origins<I>(mut self, origins: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<SharedStr>,
    {
        self.allowed_origins = origins.into_iter().map(Into::into).collect();
        self
    }

    /// Maximum request body size in bytes. Defaults to 4 MiB.
    pub fn max_request_body_bytes(mut self, bytes: usize) -> Self {
        self.max_request_body_bytes = Some(bytes);
        self
    }

    fn http_config(&self) -> StreamableHttpServerConfig {
        // Stateless: no sessions, plain JSON responses for request/response
        // tools. Clients on protocol versions with an `initialize` handshake
        // are still served; they simply receive no session ID.
        let mut config = StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true);
        if self.allow_any_host {
            config = config.disable_allowed_hosts();
        } else if let Some(hosts) = &self.allowed_hosts {
            config = config.with_allowed_hosts(hosts.iter().map(|host| host.to_string()));
        }
        if !self.allowed_origins.is_empty() {
            config = config
                .with_allowed_origins(self.allowed_origins.iter().map(|origin| origin.to_string()));
        }
        if let Some(bytes) = self.max_request_body_bytes {
            config = config.with_max_request_body_bytes(bytes);
        }
        config
    }

    fn server_info(&self) -> ServerConfig {
        let mut implementation = Implementation::new(self.name.as_ref(), self.version.as_ref());
        if let Some(title) = &self.title {
            implementation = implementation.with_title(title.as_ref());
        }
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(implementation);
        if let Some(instructions) = &self.instructions {
            info = info.with_instructions(instructions.as_ref());
        }
        info
    }
}

/// Builds an axum router serving every tool in `tool_sets` over MCP's
/// Streamable HTTP transport at [`McpConfig::path`].
///
/// Merge the router where the application's authentication layers apply.
/// Fails when two tools share a name, or when a tool name or input schema is
/// invalid.
pub fn make_router<I>(config: McpConfig, tool_sets: I) -> McpResult<axum::Router>
where
    I: IntoIterator<Item = Tools>,
{
    if !config.path.as_ref().starts_with('/') {
        return Err(UniError::from_kind_context(
            McpErrKind::InvalidConfig,
            format!("MCP path must start with '/': {:?}", config.path.as_ref()),
        ));
    }

    let mut tools = Vec::new();
    let mut handlers = HashMap::new();
    for entry in tool_sets.into_iter().flat_map(|tools| tools.entries) {
        let name = entry.tool.name.clone();
        validate_tool_name(name.as_ref())?;
        let tool = rmcp_tool(&entry.tool)?;
        if handlers
            .insert(name.clone(), (tool.clone(), entry.handler))
            .is_some()
        {
            return Err(UniError::from_kind_context(
                McpErrKind::DuplicateToolName,
                format!(
                    "MCP tool {:?} is defined more than once; rename one with the `names` section of connect2mcp.yaml",
                    name.as_ref()
                ),
            ));
        }
        tools.push(tool);
    }

    let server = McpServer {
        inner: Arc::new(ServerInner {
            info: config.server_info(),
            tools,
            handlers,
        }),
    };
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(NeverSessionManager::default()),
        config.http_config(),
    );

    Ok(axum::Router::new().route_service(config.path.as_ref(), service))
}

fn validate_tool_name(name: &str) -> McpResult<()> {
    let valid = !name.is_empty()
        && name.len() <= MAX_TOOL_NAME_LEN
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(UniError::from_kind_context(
            McpErrKind::InvalidToolName,
            format!(
                "MCP tool name {name:?} must be 1 to {MAX_TOOL_NAME_LEN} characters from A-Z, a-z, 0-9, '_', '-' and '.'"
            ),
        ))
    }
}

fn rmcp_tool(tool: &crate::Tool) -> McpResult<rmcp::model::Tool> {
    let invalid = |reason: String| {
        UniError::from_kind_context(
            McpErrKind::InvalidInputSchema,
            format!("tool {:?}: {reason}", tool.name.as_ref()),
        )
    };
    let schema = serde_json::from_str::<JsonObject>(tool.input_schema.as_ref())
        .map_err(|err| invalid(format!("input schema is not a JSON object: {err}")))?;
    if schema.get("type").and_then(serde_json::Value::as_str) != Some("object") {
        return Err(invalid(
            r#"input schema root must have "type": "object""#.to_owned(),
        ));
    }

    Ok(rmcp::model::Tool::new_with_raw(
        tool.name.to_string(),
        tool.description
            .as_ref()
            .map(|description| description.to_string().into()),
        Arc::new(schema),
    ))
}

#[derive(Clone)]
struct McpServer {
    inner: Arc<ServerInner>,
}

struct ServerInner {
    info: ServerConfig,
    tools: Vec<rmcp::model::Tool>,
    handlers: HashMap<SharedStr, (rmcp::model::Tool, BoxHandler)>,
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        self.inner.info.clone()
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.inner.tools.clone()))
    }

    fn get_tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        self.inner.handlers.get(name).map(|(tool, _)| tool.clone())
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let Some((_, handler)) = self.inner.handlers.get(request.name.as_ref()) else {
            return Err(ErrorData::invalid_params("tool not found", None));
        };

        // The HTTP transport stores the request's parts in the context, so
        // headers and extensions set by outer layers (such as an
        // authenticated identity) reach the Connect service.
        let (headers, extensions) = context
            .extensions
            .get::<http::request::Parts>()
            .map(|parts| (parts.headers.clone(), parts.extensions.clone()))
            .unwrap_or_default();
        let ctx = connect2axum::request_context(headers, extensions);
        let call = handler(ctx, request.arguments.unwrap_or_default());

        // Stop work when the client cancels or disconnects.
        let result = match select(call, Box::pin(context.ct.cancelled())).await {
            Either::Left((result, _)) => result,
            Either::Right(_) => {
                crate::error_result(connectrpc::ConnectError::canceled("tool call cancelled"))
            }
        };

        Ok(call_tool_result(result).into())
    }
}

fn call_tool_result(result: ToolResult) -> CallToolResult {
    if result.is_error {
        CallToolResult::structured_error(result.value)
    } else {
        CallToolResult::structured(result.value)
    }
}
