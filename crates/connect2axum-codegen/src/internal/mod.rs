pub mod api_names;
pub mod asyncapi;
pub mod field_options;
pub mod guardrails;
pub mod http;
pub mod ir;
#[allow(
    dead_code,
    reason = "wired to protoc-gen-connect2mcp in the next phase"
)]
pub mod mcp;
pub mod openapi;
pub mod options;
pub mod resolver;
pub mod rest;
pub mod schema;
pub mod shape;
pub mod ws;
