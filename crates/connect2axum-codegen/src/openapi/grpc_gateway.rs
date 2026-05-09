use std::env;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use buffa::Message as _;
use connectrpc_codegen::codegen::descriptor::FileDescriptorProto;
use connectrpc_codegen::plugin::CodeGeneratorResponse;
use uni_error::UniError;

use crate::CodeGeneratorRequest;
use crate::error::{CodegenErrKind, CodegenResult};

const DEFAULT_OPENAPIV3_BIN: &str = "protoc-gen-openapiv3";

pub fn openapiv3_binary(configured: Option<&Path>) -> CodegenResult<PathBuf> {
    if let Some(path) = configured {
        return Ok(path.to_path_buf());
    }
    if let Ok(path) = env::var("CONNECT2AXUM_OPENAPIV3_BIN")
        && !path.trim().is_empty()
    {
        return Ok(PathBuf::from(path));
    }
    if command_exists(DEFAULT_OPENAPIV3_BIN) {
        return Ok(PathBuf::from(DEFAULT_OPENAPIV3_BIN));
    }
    if let Some(home) = env::var_os("HOME") {
        let path = PathBuf::from(home)
            .join("go")
            .join("bin")
            .join(DEFAULT_OPENAPIV3_BIN);
        if path.is_file() {
            return Ok(path);
        }
    }

    Err(UniError::from_kind_context(
        CodegenErrKind::OpenApiPluginFailed,
        "could not find protoc-gen-openapiv3; set openapiv3_bin=... or CONNECT2AXUM_OPENAPIV3_BIN",
    ))
}

pub fn openapiv3_parameter(options: &[String]) -> String {
    let mut options = options.to_vec();
    if !options
        .iter()
        .any(|option| option.starts_with("disable_default_errors="))
    {
        options.insert(0, "disable_default_errors=true".to_owned());
    }
    options.join(",")
}

pub fn run_openapiv3(
    binary: &Path,
    request: &CodeGeneratorRequest,
) -> CodegenResult<CodeGeneratorResponse> {
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiPluginFailed,
                format!("failed to start {}: {err}", binary.display()),
            )
        })?;

    let request_bytes = request.encode_to_vec();
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(&request_bytes)
        .map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiPluginFailed,
                format!(
                    "failed to write CodeGeneratorRequest to {}: {err}",
                    binary.display()
                ),
            )
        })?;

    let output = child.wait_with_output().map_err(|err| {
        UniError::from_kind_context(
            CodegenErrKind::OpenApiPluginFailed,
            format!("failed to wait for {}: {err}", binary.display()),
        )
    })?;

    let response = if output.stdout.is_empty() {
        CodeGeneratorResponse::default()
    } else {
        CodeGeneratorResponse::decode_from_slice(&output.stdout).map_err(|err| {
            UniError::from_kind_context(
                CodegenErrKind::OpenApiPluginFailed,
                format!("failed to decode protoc-gen-openapiv3 response: {err}"),
            )
        })?
    };

    if let Some(error) = response.error.as_ref() {
        return Err(UniError::from_kind_context(
            CodegenErrKind::OpenApiPluginFailed,
            format!("protoc-gen-openapiv3 failed: {error}"),
        ));
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(UniError::from_kind_context(
            CodegenErrKind::OpenApiPluginFailed,
            format!(
                "protoc-gen-openapiv3 exited with status {}{}",
                output.status,
                if stderr.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", stderr.trim())
                }
            ),
        ));
    }

    Ok(response)
}

pub fn inject_go_packages(request: &mut CodeGeneratorRequest) {
    for file in request
        .proto_file
        .iter_mut()
        .chain(request.source_file_descriptors.iter_mut())
    {
        let options = file.options.get_or_insert_default();
        if options
            .go_package
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            continue;
        }

        let go_package = synthetic_go_package(file);
        file.options.get_or_insert_default().go_package = Some(go_package);
    }
}

fn command_exists(command: &str) -> bool {
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|path| {
            let candidate = path.join(command);
            candidate.is_file()
        })
    })
}

fn synthetic_go_package(file: &FileDescriptorProto) -> String {
    let name = file.name.as_deref().unwrap_or("schema.proto");
    let stem = name.strip_suffix(".proto").unwrap_or(name);
    let import_path = format!("connect2axum.local/gen/{}", sanitize_go_import_path(stem));
    let alias_source = file
        .package
        .as_deref()
        .and_then(|package| package.rsplit('.').next())
        .filter(|part| !part.is_empty())
        .unwrap_or(stem);
    let alias = sanitize_go_package_alias(alias_source);
    format!("{import_path};{alias}")
}

fn sanitize_go_import_path(value: &str) -> String {
    value
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn sanitize_go_package_alias(value: &str) -> String {
    let mut alias = value
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                Some(ch.to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect::<String>();

    if alias.is_empty() {
        alias.push_str("schema");
    }
    if alias.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        alias.insert(0, 'p');
    }
    alias
}
