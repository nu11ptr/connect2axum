use std::future::Future;
use std::sync::Arc;

use buffa::Message;
use connectrpc::{ConnectError, Encodable, RequestContext, ServiceResult};
use flexstr::SharedStr;
use futures_util::future::{BoxFuture, FutureExt as _};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// The description of one MCP tool: its name, optional description, and the
/// JSON Schema of its arguments.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Tool {
    pub(crate) name: SharedStr,
    pub(crate) description: Option<SharedStr>,
    pub(crate) input_schema: SharedStr,
    pub(crate) requires_confirmation: bool,
}

impl Tool {
    /// Creates a tool. `input_schema` is JSON text for a JSON Schema whose
    /// root is `{"type": "object", ...}`; it is validated by
    /// [`make_router`](crate::make_router).
    #[must_use]
    pub fn new(name: impl Into<SharedStr>, input_schema: impl Into<SharedStr>) -> Self {
        Self {
            name: name.into(),
            description: None,
            input_schema: input_schema.into(),
            requires_confirmation: false,
        }
    }

    /// Sets the description shown to the model.
    #[must_use]
    pub fn description(mut self, description: impl Into<SharedStr>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Requires the user to confirm each call before the service runs.
    ///
    /// The server asks the client to show the user the tool name and
    /// arguments, and runs the call only if the user accepts. Clients that
    /// cannot ask, which includes every client on a protocol revision before
    /// `2026-07-28`, get an error result instead, so the tool never runs
    /// unconfirmed.
    #[must_use]
    pub fn requires_confirmation(mut self) -> Self {
        self.requires_confirmation = true;
        self
    }

    /// The tool name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    /// Whether calls need the user's confirmation.
    #[must_use]
    pub fn needs_confirmation(&self) -> bool {
        self.requires_confirmation
    }
}

/// The outcome of one tool call, produced by [`tool_result`] or
/// [`error_result`].
#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub(crate) value: Value,
    pub(crate) is_error: bool,
}

/// Converts a Connect service result into a tool result.
///
/// A successful response becomes the structured result as ProtoJSON. A
/// Connect error becomes an error result carrying the Connect error JSON, so
/// the model can read the message and correct its arguments.
pub fn tool_result<M, B>(response: ServiceResult<B>) -> ToolResult
where
    M: Message + Serialize,
    B: Encodable<M>,
{
    let response = match response {
        Ok(response) => response,
        Err(err) => return error_result(err),
    };
    let json = match connect2axum::encode_json_compatible::<M, B>(&response.body) {
        Ok(json) => json,
        Err(err) => return error_result(err),
    };
    match serde_json::from_slice(&json) {
        Ok(value) => ToolResult {
            value,
            is_error: false,
        },
        Err(err) => error_result(ConnectError::internal(format!(
            "failed to read encoded tool response: {err}"
        ))),
    }
}

/// Converts a Connect error into an error tool result.
#[must_use]
pub fn error_result(err: ConnectError) -> ToolResult {
    let value = serde_json::from_slice(&err.to_json()).unwrap_or_else(
        |_| serde_json::json!({ "code": err.code.as_str(), "message": err.message }),
    );
    ToolResult {
        value,
        is_error: true,
    }
}

pub(crate) type BoxHandler =
    Arc<dyn Fn(RequestContext, Map<String, Value>) -> BoxFuture<'static, ToolResult> + Send + Sync>;

pub(crate) struct ToolEntry {
    pub(crate) tool: Tool,
    pub(crate) handler: BoxHandler,
}

/// A set of tools sharing one service, usually produced by a generated
/// `make_tools` function. Pass one or more to [`make_router`](crate::make_router).
#[must_use]
pub struct Tools {
    pub(crate) entries: Vec<ToolEntry>,
}

impl Tools {
    /// Starts a tool set whose handlers all receive `service`.
    pub fn builder<S>(service: Arc<S>) -> ToolsBuilder<S>
    where
        S: Send + Sync + 'static,
    {
        ToolsBuilder {
            service,
            entries: Vec::new(),
        }
    }

    /// The tools in this set, in registration order.
    pub fn tools(&self) -> impl Iterator<Item = &Tool> {
        self.entries.iter().map(|entry| &entry.tool)
    }
}

/// Builder returned by [`Tools::builder`].
#[must_use]
pub struct ToolsBuilder<S> {
    service: Arc<S>,
    entries: Vec<ToolEntry>,
}

impl<S> ToolsBuilder<S>
where
    S: Send + Sync + 'static,
{
    /// Adds a tool. The tool's JSON arguments are decoded into `Req` before
    /// `handler` runs; a decode failure becomes an `invalid_argument` error
    /// result without calling the handler.
    pub fn tool<Req, F, Fut>(mut self, tool: Tool, handler: F) -> Self
    where
        Req: DeserializeOwned + Send + 'static,
        F: Fn(Arc<S>, RequestContext, Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ToolResult> + Send + 'static,
    {
        let service = self.service.clone();
        let handler: BoxHandler = Arc::new(move |ctx, arguments| {
            match serde_json::from_value::<Req>(Value::Object(arguments)) {
                Ok(request) => handler(service.clone(), ctx, request).boxed(),
                Err(err) => {
                    let result = error_result(ConnectError::invalid_argument(format!(
                        "failed to decode tool arguments: {err}"
                    )));
                    async move { result }.boxed()
                }
            }
        });
        self.entries.push(ToolEntry { tool, handler });
        self
    }

    /// Finishes the tool set.
    pub fn build(self) -> Tools {
        Tools {
            entries: self.entries,
        }
    }
}
