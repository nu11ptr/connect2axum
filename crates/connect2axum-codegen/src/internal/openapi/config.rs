use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};

pub const DEFAULT_STREAMING_CONTENT_TYPE: &str = "application/x-ndjson";

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DocConfig {
    pub info: InfoConfig,
    pub servers: Vec<Value>,
    pub security_schemes: BTreeMap<String, Value>,
    pub security: Option<Value>,
    pub headers: Vec<HeaderConfig>,
    pub streaming_content_type: Option<String>,
}

impl DocConfig {
    pub fn from_path(path: &Path) -> CodegenResult<Self> {
        Self::from_path_with_kind(path, CodegenErrKind::OpenApiInvalidDocument)
    }

    pub fn from_asyncapi_path(path: &Path) -> CodegenResult<Self> {
        Self::from_path_with_kind(path, CodegenErrKind::AsyncApiInvalidDocument)
    }

    fn from_path_with_kind(path: &Path, error_kind: CodegenErrKind) -> CodegenResult<Self> {
        let content = std::fs::read_to_string(path).map_err(|err| {
            UniError::from_kind_context(
                error_kind,
                format!(
                    "failed to read API document config {}: {err}",
                    path.display()
                ),
            )
        })?;

        serde_yaml::from_str(&content).map_err(|err| {
            UniError::from_kind_context(
                error_kind,
                format!(
                    "failed to parse API document config {}: {err}",
                    path.display()
                ),
            )
        })
    }

    pub fn streaming_content_type<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.streaming_content_type.as_deref().unwrap_or(fallback)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct InfoConfig {
    pub title: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub terms_of_service: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HeaderConfig {
    pub name: String,
    pub required: bool,
    pub description: Option<String>,
    pub schema: Value,
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
