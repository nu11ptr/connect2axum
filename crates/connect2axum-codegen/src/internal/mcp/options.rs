use std::path::PathBuf;

use flexstr::{SharedStr, ToOwnedFlexStr as _};
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};
use crate::internal::options::CodegenOptions;

const DEFAULT_MCP_MODULE: &str = "::connect2axum_mcp";

/// Plugin options accepted by `protoc-gen-connect2mcp`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpOptions {
    /// Shared Rust path options, reused for type resolution.
    pub codegen: CodegenOptions,
    /// Rust path to the MCP runtime crate.
    pub mcp_module: SharedStr,
    /// Path to the YAML config file.
    pub config_path: Option<PathBuf>,
}

impl Default for McpOptions {
    fn default() -> Self {
        Self {
            codegen: CodegenOptions::default(),
            mcp_module: DEFAULT_MCP_MODULE.into(),
            config_path: None,
        }
    }
}

impl McpOptions {
    pub fn parse(parameter: Option<&str>) -> CodegenResult<Self> {
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

            match name {
                // An empty value suffix is allowed, as for REST and WS.
                "value_suffix" => options.codegen.value_suffix = value.to_owned_opt(),
                _ if value.is_empty() => {
                    return Err(invalid_option(format!("{name} cannot be empty")));
                }
                "buffa_module" => options.codegen.buffa_module = value.to_owned_opt(),
                "connect_module" => options.codegen.connect_module = value.to_owned_opt(),
                "runtime_module" => options.codegen.runtime_module = value.to_owned_opt(),
                "mcp_module" => options.mcp_module = value.to_owned_opt(),
                "config" => options.config_path = Some(PathBuf::from(value)),
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
}

fn invalid_option(context: String) -> UniError<CodegenErrKind> {
    UniError::from_kind_context(CodegenErrKind::InvalidPluginOption, context)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::McpOptions;

    #[test]
    fn defaults() {
        let options = McpOptions::parse(None).unwrap();

        assert_eq!(options.codegen.buffa_module.as_ref(), "crate::proto");
        assert_eq!(options.codegen.connect_module.as_ref(), "crate::connect");
        assert_eq!(options.codegen.runtime_module.as_ref(), "::connect2axum");
        assert_eq!(options.codegen.value_suffix.as_ref(), "__");
        assert_eq!(options.mcp_module.as_ref(), "::connect2axum_mcp");
        assert_eq!(options.config_path, None);
    }

    #[test]
    fn parses_all_options() {
        let options = McpOptions::parse(Some(
            "buffa_module=crate::p,connect_module=crate::c,runtime_module=crate::rt,\
             mcp_module=crate::mcp_rt,value_suffix=,config=connect2mcp.yaml",
        ))
        .unwrap();

        assert_eq!(options.codegen.buffa_module.as_ref(), "crate::p");
        assert_eq!(options.codegen.connect_module.as_ref(), "crate::c");
        assert_eq!(options.codegen.runtime_module.as_ref(), "crate::rt");
        assert_eq!(options.codegen.value_suffix.as_ref(), "");
        assert_eq!(options.mcp_module.as_ref(), "crate::mcp_rt");
        assert_eq!(options.config_path, Some(PathBuf::from("connect2mcp.yaml")));
    }

    #[test]
    fn rejects_unknown_rest_only_and_empty_options() {
        let err = McpOptions::parse(Some("streaming_content_type=x")).unwrap_err();
        assert!(err.to_string().contains("unknown plugin option"));

        let err = McpOptions::parse(Some("mcp_module=")).unwrap_err();
        assert!(err.to_string().contains("mcp_module cannot be empty"));
    }
}
