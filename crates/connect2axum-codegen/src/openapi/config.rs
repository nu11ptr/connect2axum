use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};

pub(crate) const DEFAULT_STREAMING_CONTENT_TYPE: &str = "application/x-ndjson";

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct DocConfig {
    pub info: InfoConfig,
    pub servers: Vec<Value>,
    pub security_schemes: BTreeMap<String, Value>,
    pub security: Option<Value>,
    pub headers: Vec<HeaderConfig>,
    pub streaming_content_type: Option<String>,
    pub default_error_response: Option<bool>,
}

impl DocConfig {
    pub(crate) fn from_path(path: &Path) -> CodegenResult<Self> {
        let content = std::fs::read_to_string(path).map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiInvalidDocument,
                format!(
                    "failed to read API document config {}: {err}",
                    path.display()
                ),
            )
        })?;

        serde_yaml::from_str(&content).map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiInvalidDocument,
                format!(
                    "failed to parse API document config {}: {err}",
                    path.display()
                ),
            )
        })
    }

    pub(crate) fn streaming_content_type<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.streaming_content_type.as_deref().unwrap_or(fallback)
    }

    pub(crate) fn add_default_error_response(&self) -> bool {
        self.default_error_response.unwrap_or(true)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct InfoConfig {
    pub title: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub terms_of_service: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct HeaderConfig {
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
