use std::borrow::Cow;

use uni_error::{Cause, UniKind};

/// Error categories produced while assembling an MCP router.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum McpErrKind {
    /// Two tools, possibly from different services, have the same name.
    DuplicateToolName,
    /// A tool name is empty, too long, or uses characters MCP does not allow.
    InvalidToolName,
    /// A tool's input schema is not a JSON object schema.
    InvalidInputSchema,
    /// A configuration value is invalid.
    InvalidConfig,
}

impl UniKind for McpErrKind {
    fn context(&self, _cause: Option<Cause<'_>>) -> Option<Cow<'static, str>> {
        match self {
            Self::DuplicateToolName => Some("duplicate MCP tool name".into()),
            Self::InvalidToolName => Some("invalid MCP tool name".into()),
            Self::InvalidInputSchema => Some("invalid MCP tool input schema".into()),
            Self::InvalidConfig => Some("invalid MCP configuration".into()),
        }
    }
}

/// Result type for fallible `connect2axum-mcp` functions.
pub type McpResult<T> = uni_error::UniResult<T, McpErrKind>;
