use buffa::bytes::Buf as _;
use buffa::encoding::{Tag, WireType, skip_field_depth};
use buffa::unknown_fields::UnknownFieldData;
use connectrpc_codegen::codegen::descriptor::MethodDescriptorProto;
use flexstr::ToOwnedFlexStr as _;
use uni_error::{ResultContext as _, UniError};

use crate::error::{CodegenErrKind, CodegenResult};
use crate::internal::ir::{HttpBinding, HttpBody, HttpVerb};

pub const HTTP_EXTENSION_NUMBER: u32 = 72_295_728;

pub fn extract_http_binding(method: &MethodDescriptorProto) -> CodegenResult<Option<HttpBinding>> {
    let Some(options) = method.options.as_option() else {
        return Ok(None);
    };

    for unknown_field in options.__buffa_unknown_fields.iter() {
        if unknown_field.number != HTTP_EXTENSION_NUMBER {
            continue;
        }

        let UnknownFieldData::LengthDelimited(bytes) = &unknown_field.data else {
            return Err(UniError::from_kind_context(
                CodegenErrKind::InvalidHttpAnnotation,
                "google.api.http extension must be length-delimited",
            ));
        };

        return parse_http_rule(bytes);
    }

    Ok(None)
}

fn parse_http_rule(bytes: &[u8]) -> CodegenResult<Option<HttpBinding>> {
    let mut bytes = bytes;
    let mut verb = None;
    let mut path = None;
    let mut body = HttpBody::None;

    while bytes.has_remaining() {
        let tag = Tag::decode(&mut bytes).kind_context(
            CodegenErrKind::InvalidHttpAnnotation,
            "failed to decode google.api.http rule tag",
        )?;

        match tag.field_number() {
            2 => set_pattern(&mut verb, &mut path, HttpVerb::Get, tag, &mut bytes)?,
            3 => set_pattern(&mut verb, &mut path, HttpVerb::Put, tag, &mut bytes)?,
            4 => set_pattern(&mut verb, &mut path, HttpVerb::Post, tag, &mut bytes)?,
            5 => set_pattern(&mut verb, &mut path, HttpVerb::Delete, tag, &mut bytes)?,
            6 => set_pattern(&mut verb, &mut path, HttpVerb::Patch, tag, &mut bytes)?,
            7 => {
                let body_value = decode_string_field(tag, &mut bytes, "body")?;
                body = parse_body(&body_value)?;
            }
            8 => {
                return Err(UniError::from_kind_context(
                    CodegenErrKind::UnsupportedHttpRule,
                    "custom google.api.http verbs are not supported yet",
                ));
            }
            _ => skip_unknown(tag, &mut bytes)?,
        }
    }

    let Some(verb) = verb else {
        return Ok(None);
    };
    let path = path.ok_or_else(|| {
        UniError::from_kind_context(
            CodegenErrKind::InvalidHttpAnnotation,
            "google.api.http rule had a verb but no path",
        )
    })?;
    let path_variables = parse_path_variables(&path)?;

    Ok(Some(HttpBinding {
        verb,
        path: path.to_owned_opt(),
        body,
        path_variables,
    }))
}

fn set_pattern(
    verb: &mut Option<HttpVerb>,
    path: &mut Option<String>,
    next_verb: HttpVerb,
    tag: Tag,
    bytes: &mut &[u8],
) -> CodegenResult<()> {
    *verb = Some(next_verb);
    *path = Some(decode_string_field(tag, bytes, next_verb.as_str())?);
    Ok(())
}

fn parse_body(value: &str) -> CodegenResult<HttpBody> {
    if value == "*" {
        Ok(HttpBody::Wildcard)
    } else if is_simple_field_name(value) {
        Ok(HttpBody::Field(value.to_owned_opt()))
    } else {
        Err(UniError::from_kind_context(
            CodegenErrKind::UnsupportedHttpRule,
            format!("unsupported google.api.http body mapping: {value}"),
        ))
    }
}

fn parse_path_variables(path: &str) -> CodegenResult<Vec<flexstr::SharedStr>> {
    if !path.starts_with('/') {
        return Err(UniError::from_kind_context(
            CodegenErrKind::UnsupportedHttpRule,
            format!("unsupported google.api.http path template: {path}"),
        ));
    }

    let mut variables = Vec::new();
    for segment in path.split('/') {
        if segment == "*" || segment == "**" {
            return Err(UniError::from_kind_context(
                CodegenErrKind::UnsupportedHttpRule,
                format!("unsupported google.api.http path template: {path}"),
            ));
        }

        let has_variable_syntax = segment.contains('{') || segment.contains('}');
        if !has_variable_syntax {
            continue;
        }

        let Some(field_name) = segment
            .strip_prefix('{')
            .and_then(|segment| segment.strip_suffix('}'))
        else {
            return Err(UniError::from_kind_context(
                CodegenErrKind::UnsupportedHttpRule,
                format!("unsupported google.api.http path template: {path}"),
            ));
        };

        if !is_simple_field_name(field_name) {
            return Err(UniError::from_kind_context(
                CodegenErrKind::UnsupportedHttpRule,
                format!("unsupported google.api.http path template: {path}"),
            ));
        }

        variables.push(field_name.to_owned_opt());
    }

    Ok(variables)
}

fn is_simple_field_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    (first == '_' || first.is_ascii_lowercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

fn decode_string_field(tag: Tag, bytes: &mut &[u8], field_name: &str) -> CodegenResult<String> {
    if tag.wire_type() != WireType::LengthDelimited {
        return Err(UniError::from_kind_context(
            CodegenErrKind::InvalidHttpAnnotation,
            format!("google.api.http {field_name} field must be length-delimited"),
        ));
    }

    buffa::types::decode_string(bytes).kind_context(
        CodegenErrKind::InvalidHttpAnnotation,
        format!("failed to decode google.api.http {field_name} field"),
    )
}

fn skip_unknown(tag: Tag, bytes: &mut &[u8]) -> CodegenResult<()> {
    skip_field_depth(tag, bytes, buffa::message::RECURSION_LIMIT).kind_context(
        CodegenErrKind::InvalidHttpAnnotation,
        "failed to skip unsupported google.api.http field",
    )
}

#[cfg(test)]
mod tests {
    use buffa::encoding::{Tag, WireType};
    use buffa::{MessageField, UnknownField, UnknownFieldData};
    use connectrpc_codegen::codegen::descriptor::{MethodDescriptorProto, MethodOptions};

    use super::{HTTP_EXTENSION_NUMBER, extract_http_binding};
    use crate::internal::ir::{HttpBody, HttpVerb};

    #[test]
    fn extracts_http_extension_from_method_options_unknown_fields() {
        let method = method_with_http(http_rule(6, "/v1/{resource_id}", Some("*")));

        let binding = extract_http_binding(&method).unwrap().unwrap();

        assert_eq!(binding.verb, HttpVerb::Patch);
        assert_eq!(binding.path.as_ref(), "/v1/{resource_id}");
        assert_eq!(binding.body, HttpBody::Wildcard);
        assert_eq!(
            binding
                .path_variables
                .iter()
                .map(|field| field.as_ref())
                .collect::<Vec<_>>(),
            vec!["resource_id"]
        );
    }

    #[test]
    fn no_http_extension_returns_none() {
        let method = MethodDescriptorProto::default();

        let binding = extract_http_binding(&method).unwrap();

        assert_eq!(binding, None);
    }

    #[test]
    fn rejects_nested_path_variables() {
        let method = method_with_http(http_rule(2, "/v1/{message.name}", None));

        let err = extract_http_binding(&method).unwrap_err();

        assert!(
            err.to_string()
                .contains("unsupported google.api.http path template")
        );
    }

    fn method_with_http(http_rule: Vec<u8>) -> MethodDescriptorProto {
        let mut options = MethodOptions::default();
        options.__buffa_unknown_fields.push(UnknownField {
            number: HTTP_EXTENSION_NUMBER,
            data: UnknownFieldData::LengthDelimited(http_rule),
        });

        MethodDescriptorProto {
            options: MessageField::some(options),
            ..Default::default()
        }
    }

    fn http_rule(verb_field: u32, path: &str, body: Option<&str>) -> Vec<u8> {
        let mut bytes = Vec::new();
        Tag::new(verb_field, WireType::LengthDelimited).encode(&mut bytes);
        buffa::types::encode_string(path, &mut bytes);
        if let Some(body) = body {
            Tag::new(7, WireType::LengthDelimited).encode(&mut bytes);
            buffa::types::encode_string(body, &mut bytes);
        }
        bytes
    }
}
