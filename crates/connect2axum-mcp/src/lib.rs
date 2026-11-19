//! Runtime for generated MCP (Model Context Protocol) wrappers over
//! ConnectRPC services.
//!
//! `protoc-gen-connect2mcp` generates one `<service>_mcp` module per service,
//! with a `make_tools` function returning that service's [`Tools`]. Pass the
//! tool sets to [`make_router`] to get an `axum::Router` that serves them over
//! MCP's Streamable HTTP transport:
//!
//! ```ignore
//! let mcp = connect2axum_mcp::make_router(
//!     connect2axum_mcp::McpConfig::new("hello", env!("CARGO_PKG_VERSION"))
//!         .allowed_hosts(["api.example.com"]),
//!     [mcp::greeter_service_mcp::make_tools(greeter.clone())],
//! )?;
//! let app = axum::Router::new().merge(mcp);
//! ```
//!
//! The server is stateless. Authentication belongs in an HTTP layer in front
//! of the MCP route: a handler that returns `unauthenticated` produces an
//! in-band tool error, not an HTTP 401, and `tools/list` never reaches a
//! handler. For clients that log in through OAuth, [`ProtectedResource`] and
//! [`protected_resource_router`] publish where tokens come from; validating
//! tokens stays with the application.

mod auth;
mod error;
mod server;
mod tool;

pub use auth::{
    ProtectedResource, bearer_challenge, insufficient_scope_challenge, protected_resource_router,
};
pub use error::{McpErrKind, McpResult};
pub use server::{McpConfig, make_router};
pub use tool::{Tool, ToolResult, Tools, ToolsBuilder, error_result, tool_result};

/// The crate version, as declared by Cargo.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
