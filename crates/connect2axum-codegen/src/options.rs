use flexstr::{SharedStr, ToOwnedFlexStr as _};
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};

const DEFAULT_BUFFA_MODULE: &str = "crate::proto";
const DEFAULT_CONNECT_MODULE: &str = "crate::connect";
const DEFAULT_RUNTIME_MODULE: &str = "::connect2axum";
const DEFAULT_STREAMING_CONTENT_TYPE: &str = "application/x-ndjson";
const DEFAULT_VALUE_SUFFIX: &str = "__";
const DEFAULT_TYPE_SUFFIX: &str = "__";
const DEFAULT_BODY_MESSAGE_SUFFIX: &str = "Body";
const DEFAULT_QUERY_MESSAGE_SUFFIX: &str = "Query";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceState {
    pub service: SharedStr,
    pub state_type: SharedStr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodegenOptions {
    pub buffa_module: SharedStr,
    pub connect_module: SharedStr,
    pub runtime_module: SharedStr,
    pub openapi: bool,
    pub streaming_content_type: SharedStr,
    pub value_suffix: SharedStr,
    pub type_suffix: SharedStr,
    pub body_message_suffix: SharedStr,
    pub query_message_suffix: SharedStr,
    pub service_states: Vec<ServiceState>,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            buffa_module: DEFAULT_BUFFA_MODULE.into(),
            connect_module: DEFAULT_CONNECT_MODULE.into(),
            runtime_module: DEFAULT_RUNTIME_MODULE.into(),
            openapi: false,
            streaming_content_type: DEFAULT_STREAMING_CONTENT_TYPE.into(),
            value_suffix: DEFAULT_VALUE_SUFFIX.into(),
            type_suffix: DEFAULT_TYPE_SUFFIX.into(),
            body_message_suffix: DEFAULT_BODY_MESSAGE_SUFFIX.into(),
            query_message_suffix: DEFAULT_QUERY_MESSAGE_SUFFIX.into(),
            service_states: Vec::new(),
        }
    }
}

impl CodegenOptions {
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
                "buffa_module" => options.buffa_module = parse_non_empty(name, value)?,
                "connect_module" => options.connect_module = parse_non_empty(name, value)?,
                "runtime_module" => options.runtime_module = parse_non_empty(name, value)?,
                "openapi" => options.openapi = parse_bool(name, value)?,
                "streaming_content_type" => {
                    options.streaming_content_type = parse_non_empty(name, value)?;
                }
                "value_suffix" => options.value_suffix = value.to_owned_opt(),
                "type_suffix" => options.type_suffix = value.to_owned_opt(),
                "body_message_suffix" => {
                    options.body_message_suffix = parse_non_empty(name, value)?;
                }
                "query_message_suffix" => {
                    options.query_message_suffix = parse_non_empty(name, value)?;
                }
                "service_state" => options.service_states.push(parse_service_state(value)?),
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

fn parse_non_empty(name: &str, value: &str) -> CodegenResult<SharedStr> {
    if value.is_empty() {
        Err(invalid_option(format!("{name} cannot be empty")))
    } else {
        Ok(value.to_owned_opt())
    }
}

fn parse_bool(name: &str, value: &str) -> CodegenResult<bool> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(UniError::from_kind_context(
            CodegenErrKind::InvalidBooleanOption,
            format!("{name} must be true or false"),
        )),
    }
}

fn parse_service_state(value: &str) -> CodegenResult<ServiceState> {
    let (service, state_type) = value.split_once(':').ok_or_else(|| {
        UniError::from_kind_context(
            CodegenErrKind::InvalidServiceStateOption,
            "service_state must use service.fqn:RustType syntax",
        )
    })?;
    let service = service.trim();
    let state_type = state_type.trim();

    if service.is_empty() || state_type.is_empty() {
        Err(UniError::from_kind_context(
            CodegenErrKind::InvalidServiceStateOption,
            "service_state service and Rust type must both be non-empty",
        ))
    } else {
        Ok(ServiceState {
            service: service.to_owned_opt(),
            state_type: state_type.to_owned_opt(),
        })
    }
}

fn invalid_option(context: String) -> uni_error::UniError<CodegenErrKind> {
    UniError::from_kind_context(CodegenErrKind::InvalidPluginOption, context)
}

#[cfg(test)]
mod tests {
    use super::{CodegenOptions, ServiceState};

    #[test]
    fn defaults_match_phase_2_contract() {
        let options = CodegenOptions::parse(None).unwrap();

        assert_eq!(options.buffa_module.as_ref(), "crate::proto");
        assert_eq!(options.connect_module.as_ref(), "crate::connect");
        assert_eq!(options.runtime_module.as_ref(), "::connect2axum");
        assert!(!options.openapi);
        assert_eq!(
            options.streaming_content_type.as_ref(),
            "application/x-ndjson"
        );
        assert_eq!(options.value_suffix.as_ref(), "__");
        assert_eq!(options.type_suffix.as_ref(), "__");
        assert_eq!(options.body_message_suffix.as_ref(), "Body");
        assert_eq!(options.query_message_suffix.as_ref(), "Query");
        assert!(options.service_states.is_empty());
    }

    #[test]
    fn parses_all_phase_2_options() {
        let options = CodegenOptions::parse(Some(
            "buffa_module=crate::generated::proto,\
             connect_module=crate::generated::connect,\
             runtime_module=crate::runtime,\
             openapi=true,\
             streaming_content_type=application/json-seq,\
             value_suffix=_cx,\
             type_suffix=_Rest,\
             body_message_suffix=Payload,\
             query_message_suffix=Params,\
             service_state=hello.v1.Greeter:crate::Greeter",
        ))
        .unwrap();

        assert_eq!(options.buffa_module.as_ref(), "crate::generated::proto");
        assert_eq!(options.connect_module.as_ref(), "crate::generated::connect");
        assert_eq!(options.runtime_module.as_ref(), "crate::runtime");
        assert!(options.openapi);
        assert_eq!(
            options.streaming_content_type.as_ref(),
            "application/json-seq"
        );
        assert_eq!(options.value_suffix.as_ref(), "_cx");
        assert_eq!(options.type_suffix.as_ref(), "_Rest");
        assert_eq!(options.body_message_suffix.as_ref(), "Payload");
        assert_eq!(options.query_message_suffix.as_ref(), "Params");
        assert_eq!(
            options.service_states,
            vec![ServiceState {
                service: "hello.v1.Greeter".into(),
                state_type: "crate::Greeter".into(),
            }]
        );
    }

    #[test]
    fn rejects_unknown_options() {
        let err = CodegenOptions::parse(Some("surprise=true")).unwrap_err();
        assert!(err.to_string().contains("unknown plugin option: surprise"));
    }

    #[test]
    fn rejects_invalid_boolean_options() {
        let err = CodegenOptions::parse(Some("openapi=yes")).unwrap_err();
        assert!(err.to_string().contains("openapi must be true or false"));
    }

    #[test]
    fn rejects_invalid_service_state_options() {
        let err = CodegenOptions::parse(Some("service_state=hello.v1.Greeter")).unwrap_err();
        assert!(
            err.to_string()
                .contains("service_state must use service.fqn:RustType syntax")
        );
    }
}
