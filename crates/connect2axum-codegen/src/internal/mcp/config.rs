use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};

/// The `connect2mcp.yaml` file: which RPCs become tools and what they are
/// called. Entries are fully-qualified services (`package.Service`) or
/// methods (`package.Service.Method`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct McpConfigFile {
    /// When present, only these services and methods are exposed.
    pub include: Option<Vec<String>>,
    /// Always removed, with or without `include`.
    pub exclude: Vec<String>,
    /// Tool name overrides. A service key replaces the service part of the
    /// default name; a method key replaces the whole name.
    pub names: BTreeMap<String, String>,
}

impl McpConfigFile {
    pub fn from_path(path: &Path) -> CodegenResult<Self> {
        let content = std::fs::read_to_string(path).map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::InvalidMcpConfig,
                format!("failed to read MCP config {}: {err}", path.display()),
            )
        })?;
        Self::from_yaml(&content).map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::InvalidMcpConfig,
                format!("failed to parse MCP config {}: {err}", path.display()),
            )
        })
    }

    pub fn from_yaml(content: &str) -> Result<Self, serde_yaml::Error> {
        if content.trim().is_empty() {
            return Ok(Self::default());
        }
        serde_yaml::from_str(content)
    }
}

#[cfg(test)]
mod tests {
    use super::McpConfigFile;

    #[test]
    fn parses_all_sections() {
        let config = McpConfigFile::from_yaml(
            "include:\n  - a.v1.S\nexclude:\n  - a.v1.S.Delete\nnames:\n  a.v1.S: Short\n",
        )
        .unwrap();

        assert_eq!(config.include, Some(vec!["a.v1.S".to_owned()]));
        assert_eq!(config.exclude, vec!["a.v1.S.Delete".to_owned()]);
        assert_eq!(config.names["a.v1.S"], "Short");
    }

    #[test]
    fn empty_file_is_default_and_typos_are_rejected() {
        assert_eq!(
            McpConfigFile::from_yaml("").unwrap(),
            McpConfigFile::default()
        );
        assert!(McpConfigFile::from_yaml("excludes:\n  - a.v1.S\n").is_err());
    }
}
