//! Decides which RPCs become MCP tools, what they are called, and what their
//! input schemas are.

use std::collections::BTreeSet;

use flexstr::{SharedStr, ToOwnedFlexStr as _};
use serde_json::Value;
use uni_error::UniError;

use super::config::McpConfigFile;
use crate::error::{CodegenErrKind, CodegenResult};
use crate::internal::guardrails::ensure_unique_tool_names;
use crate::internal::ir::{DescriptorIr, Method, ProtoFile, Service};
use crate::internal::openapi::comments::comment_description;
use crate::internal::schema::{SchemaBuilder, is_json_object_message};

const MAX_TOOL_NAME_LEN: usize = 128;

/// Every tool in one plugin run, grouped by file and service.
pub struct Catalog<'a> {
    pub files: Vec<CatalogFile<'a>>,
}

pub struct CatalogFile<'a> {
    pub file: &'a ProtoFile,
    pub services: Vec<CatalogService<'a>>,
}

pub struct CatalogService<'a> {
    pub service: &'a Service,
    pub tools: Vec<CatalogTool<'a>>,
}

pub struct CatalogTool<'a> {
    pub method: &'a Method,
    pub name: SharedStr,
    pub description: Option<String>,
    pub input_schema: Value,
}

pub fn build_catalog<'a>(
    ir: &'a DescriptorIr,
    config: &McpConfigFile,
) -> CodegenResult<Catalog<'a>> {
    let mut files = Vec::new();
    let mut known = BTreeSet::new();

    for file_name in &ir.files_to_generate {
        let file = ir.file(file_name.as_ref()).ok_or_else(|| {
            UniError::from_kind_context(
                CodegenErrKind::FileToGenerateNotFound,
                format!("file_to_generate {file_name:?} was not present in proto_file"),
            )
        })?;

        let mut services = Vec::new();
        for service in &file.services {
            known.insert(service.full_name.as_ref());
            let mut tools = Vec::new();
            for method in &service.methods {
                known.insert(method.full_name.as_ref());
                if let Some(tool) = catalog_tool(ir, config, service, method)? {
                    tools.push(tool);
                }
            }
            if !tools.is_empty() {
                services.push(CatalogService { service, tools });
            }
        }
        if !services.is_empty() {
            files.push(CatalogFile { file, services });
        }
    }

    check_entries_matched(config, &known)?;
    ensure_unique_tool_names(
        "MCP tools",
        files
            .iter()
            .flat_map(|file| &file.services)
            .flat_map(|service| &service.tools)
            .map(|tool| {
                (
                    tool.name.clone(),
                    format!("method {}", tool.method.full_name.as_ref()),
                )
            }),
    )?;

    Ok(Catalog { files })
}

fn catalog_tool<'a>(
    ir: &'a DescriptorIr,
    config: &McpConfigFile,
    service: &'a Service,
    method: &'a Method,
) -> CodegenResult<Option<CatalogTool<'a>>> {
    let service_name = service.full_name.as_ref();
    let method_name = method.full_name.as_ref();
    let selects = |entries: &[String]| {
        entries
            .iter()
            .any(|entry| entry == service_name || entry == method_name)
    };

    let included = config.include.as_deref().is_none_or(selects);
    if !included || selects(&config.exclude) {
        return Ok(None);
    }

    if method.client_streaming || method.server_streaming {
        let explicitly_included = config
            .include
            .as_deref()
            .is_some_and(|include| include.iter().any(|entry| entry == method_name));
        if explicitly_included {
            return Err(UniError::from_kind_context(
                CodegenErrKind::InvalidMcpConfig,
                format!(
                    "MCP config includes streaming method {method_name}; MCP tools support unary RPCs only"
                ),
            ));
        }
        eprintln!(
            "warning: connect2mcp skipping streaming method {method_name}; MCP tools support unary RPCs only"
        );
        return Ok(None);
    }

    if !is_json_object_message(method.input_type.as_ref()) {
        return Err(UniError::from_kind_context(
            CodegenErrKind::UnsupportedMcpRequest,
            format!(
                "method {method_name} takes {}, whose JSON form is not an object; MCP tool arguments must be an object, so exclude this method in the MCP config",
                method.input_type.as_ref()
            ),
        ));
    }

    let name = tool_name(config, service, method)?;
    Ok(Some(CatalogTool {
        method,
        name,
        description: comment_description(&method.comments),
        input_schema: SchemaBuilder::inline_root(ir, method.input_type.as_ref())?,
    }))
}

fn tool_name(
    config: &McpConfigFile,
    service: &Service,
    method: &Method,
) -> CodegenResult<SharedStr> {
    let name = match config.names.get(method.full_name.as_ref()) {
        Some(name) => name.clone(),
        None => {
            let service_part = config
                .names
                .get(service.full_name.as_ref())
                .map_or(service.name.as_ref(), String::as_str);
            format!("{service_part}_{}", method.name.as_ref())
        }
    };

    let valid = !name.is_empty()
        && name.len() <= MAX_TOOL_NAME_LEN
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if !valid {
        return Err(UniError::from_kind_context(
            CodegenErrKind::InvalidToolName,
            format!(
                "MCP tool name {name:?} for method {} must be 1 to {MAX_TOOL_NAME_LEN} characters from A-Z, a-z, 0-9, '_', '-' and '.'",
                method.full_name.as_ref()
            ),
        ));
    }

    Ok(name.to_owned_opt())
}

fn check_entries_matched(config: &McpConfigFile, known: &BTreeSet<&str>) -> CodegenResult<()> {
    let entries = config
        .include
        .iter()
        .flatten()
        .map(|entry| ("include", entry.as_str()))
        .chain(
            config
                .exclude
                .iter()
                .map(|entry| ("exclude", entry.as_str())),
        )
        .chain(config.names.keys().map(|entry| ("names", entry.as_str())));

    for (section, entry) in entries {
        if !known.contains(entry) {
            return Err(UniError::from_kind_context(
                CodegenErrKind::UnmatchedMcpConfigEntry,
                format!(
                    "MCP config `{section}` entry {entry:?} matches no service or method in the files being generated. \
                     Entries must be fully-qualified (package.Service or package.Service.Method), and a plugin run only \
                     sees the files Buf passes it: keep one config per buf.gen.yaml and use `strategy: all` when it covers \
                     more than one directory"
                ),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use flexstr::ToOwnedFlexStr as _;

    use super::{McpConfigFile, build_catalog};
    use crate::internal::ir::{
        CommentSet, DescriptorIr, Field, FieldKind, Message, Method, ProtoFile, Service,
    };

    fn method(service: &str, name: &str, input: &str, streaming: bool) -> Method {
        Method {
            name: name.to_owned_opt(),
            full_name: format!("{service}.{name}").to_owned_opt(),
            input_type: input.to_owned_opt(),
            output_type: "a.v1.Reply".into(),
            client_streaming: false,
            server_streaming: streaming,
            comments: CommentSet {
                leading: Some(format!("{name} docs.\n").to_owned_opt()),
                ..Default::default()
            },
            http: None,
        }
    }

    fn service(full_name: &str, methods: &[(&str, &str, bool)]) -> Service {
        Service {
            name: full_name.rsplit('.').next().unwrap().to_owned_opt(),
            full_name: full_name.to_owned_opt(),
            comments: CommentSet::default(),
            methods: methods
                .iter()
                .map(|(name, input, streaming)| method(full_name, name, input, *streaming))
                .collect(),
        }
    }

    fn file(name: &str, package: &str, services: Vec<Service>) -> ProtoFile {
        ProtoFile {
            name: name.to_owned_opt(),
            package: package.to_owned_opt(),
            messages: vec![Message {
                name: "Request".into(),
                full_name: format!("{package}.Request").to_owned_opt(),
                fields: vec![Field {
                    name: "id".into(),
                    json_name: "id".into(),
                    number: Some(1),
                    kind: FieldKind::String,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            services,
            ..Default::default()
        }
    }

    fn ir_of(files: Vec<ProtoFile>) -> DescriptorIr {
        DescriptorIr {
            files_to_generate: files.iter().map(|file| file.name.clone()).collect(),
            files,
            ..Default::default()
        }
    }

    fn default_ir() -> DescriptorIr {
        ir_of(vec![file(
            "a.proto",
            "a.v1",
            vec![
                service(
                    "a.v1.Users",
                    &[
                        ("GetUser", "a.v1.Request", false),
                        ("DeleteUser", "a.v1.Request", false),
                        ("WatchUsers", "a.v1.Request", true),
                    ],
                ),
                service("a.v1.Admin", &[("Reset", "google.protobuf.Empty", false)]),
            ],
        )])
    }

    fn names(ir: &DescriptorIr, config: &McpConfigFile) -> Vec<String> {
        build_catalog(ir, config)
            .unwrap()
            .files
            .iter()
            .flat_map(|file| &file.services)
            .flat_map(|service| &service.tools)
            .map(|tool| tool.name.to_string())
            .collect()
    }

    fn config(yaml: &str) -> McpConfigFile {
        McpConfigFile::from_yaml(yaml).unwrap()
    }

    #[test]
    fn exposes_every_unary_rpc_by_default() {
        let ir = default_ir();
        let catalog = build_catalog(&ir, &McpConfigFile::default()).unwrap();
        let tool = &catalog.files[0].services[0].tools[0];

        assert_eq!(
            names(&ir, &McpConfigFile::default()),
            vec!["Users_GetUser", "Users_DeleteUser", "Admin_Reset"]
        );
        assert_eq!(tool.description.as_deref(), Some("GetUser docs."));
        assert_eq!(tool.input_schema["properties"]["id"]["type"], "string");
    }

    #[test]
    fn exclude_removes_methods_and_services() {
        let ir = default_ir();

        assert_eq!(
            names(&ir, &config("exclude:\n  - a.v1.Users.DeleteUser\n")),
            vec!["Users_GetUser", "Admin_Reset"]
        );
        assert_eq!(
            names(&ir, &config("exclude:\n  - a.v1.Users\n")),
            vec!["Admin_Reset"]
        );
    }

    #[test]
    fn include_switches_to_allowlist_and_exclude_still_applies() {
        let ir = default_ir();

        assert_eq!(
            names(
                &ir,
                &config("include:\n  - a.v1.Users\nexclude:\n  - a.v1.Users.DeleteUser\n")
            ),
            vec!["Users_GetUser"]
        );
        assert_eq!(
            names(&ir, &config("include:\n  - a.v1.Admin.Reset\n")),
            vec!["Admin_Reset"]
        );
    }

    #[test]
    fn explicitly_included_streaming_method_is_an_error() {
        let ir = default_ir();
        let err = build_catalog(&ir, &config("include:\n  - a.v1.Users.WatchUsers\n"))
            .err()
            .unwrap();

        assert!(
            err.to_string()
                .contains("streaming method a.v1.Users.WatchUsers")
        );
    }

    #[test]
    fn names_override_service_part_or_whole_name() {
        let ir = default_ir();

        assert_eq!(
            names(
                &ir,
                &config("names:\n  a.v1.Users: People\n  a.v1.Admin.Reset: reset_everything\n")
            ),
            vec!["People_GetUser", "People_DeleteUser", "reset_everything"]
        );
    }

    #[test]
    fn unmatched_entries_are_errors() {
        let ir = default_ir();
        for yaml in [
            "include:\n  - a.v1.Nope\n",
            "exclude:\n  - a.v1.Users.Gone\n",
            "names:\n  Users: X\n",
        ] {
            let err = build_catalog(&ir, &config(yaml)).err().unwrap();
            assert!(
                err.to_string().contains("matches no service or method"),
                "{err}"
            );
        }
    }

    #[test]
    fn invalid_and_duplicate_names_are_errors() {
        let ir = default_ir();
        let err = build_catalog(&ir, &config("names:\n  a.v1.Users.GetUser: get user\n"))
            .err()
            .unwrap();
        assert!(err.to_string().contains("\"get user\""), "{err}");

        let err = build_catalog(&ir, &config("names:\n  a.v1.Admin.Reset: Users_GetUser\n"))
            .err()
            .unwrap();
        assert!(
            err.to_string()
                .contains("duplicate MCP tool name \"Users_GetUser\""),
            "{err}"
        );

        let two_packages = ir_of(vec![
            file(
                "a.proto",
                "a.v1",
                vec![service("a.v1.Admin", &[("Reset", "a.v1.Request", false)])],
            ),
            file(
                "b.proto",
                "b.v1",
                vec![service("b.v1.Admin", &[("Reset", "b.v1.Request", false)])],
            ),
        ]);
        let err = build_catalog(&two_packages, &McpConfigFile::default())
            .err()
            .unwrap();
        assert!(err.to_string().contains("Admin_Reset"), "{err}");
        assert_eq!(
            names(&two_packages, &config("names:\n  b.v1.Admin: BAdmin\n")),
            vec!["Admin_Reset", "BAdmin_Reset"]
        );
    }

    #[test]
    fn non_object_request_messages_are_errors_unless_excluded() {
        let ir = ir_of(vec![file(
            "a.proto",
            "a.v1",
            vec![service(
                "a.v1.Echo",
                &[("Say", "google.protobuf.StringValue", false)],
            )],
        )]);

        let err = build_catalog(&ir, &McpConfigFile::default()).err().unwrap();
        assert!(err.to_string().contains("exclude this method"), "{err}");
        assert!(names(&ir, &config("exclude:\n  - a.v1.Echo.Say\n")).is_empty());
    }

    #[test]
    fn files_without_http_bindings_still_produce_tools() {
        let ir = default_ir();
        assert!(
            ir.files[0]
                .services
                .iter()
                .all(|service| !service.has_http_bindings())
        );

        let catalog = build_catalog(&ir, &McpConfigFile::default()).unwrap();

        assert_eq!(catalog.files[0].file.name.as_ref(), "a.proto");
    }
}
