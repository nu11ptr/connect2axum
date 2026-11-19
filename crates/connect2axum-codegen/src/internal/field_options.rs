//! Reads the field and oneof annotations that mark protobuf input as required.
//!
//! proto3 has no `required` keyword, so connect2axum honors two widely used
//! annotations instead. Both are read from the options' unknown fields, the
//! same way `google.api.http` is read in `internal/http.rs`, so no proto files
//! need to be vendored:
//!
//! - `google.api.field_behavior` (`FieldOptions` extension 1052, a repeated
//!   `FieldBehavior` enum) containing `REQUIRED` (2).
//! - Protovalidate `(buf.validate.field).required` (`FieldOptions` extension
//!   1159 holding `FieldRules`, whose `required` is field 25) and
//!   `(buf.validate.oneof).required` (`OneofOptions` extension 1159 holding
//!   `OneofRules`, whose `required` is field 1).

use buffa::bytes::Buf as _;
use buffa::encoding::{Tag, WireType, decode_varint, skip_field_depth};
use buffa::unknown_fields::{UnknownFieldData, UnknownFields};
use connectrpc_codegen::codegen::descriptor::{FieldOptions, OneofOptions};
use uni_error::{ResultContext as _, UniError};

use crate::error::{CodegenErrKind, CodegenResult};

/// `google.api.field_behavior` extension number on `FieldOptions`.
pub const FIELD_BEHAVIOR_EXTENSION_NUMBER: u32 = 1052;
/// `google.api.FieldBehavior.REQUIRED`.
const FIELD_BEHAVIOR_REQUIRED: u64 = 2;
/// Protovalidate's `buf.validate.field` / `buf.validate.oneof` extension number.
pub const PROTOVALIDATE_EXTENSION_NUMBER: u32 = 1159;
/// `buf.validate.FieldRules.required`.
const FIELD_RULES_REQUIRED: u32 = 25;
/// `buf.validate.OneofRules.required`.
const ONEOF_RULES_REQUIRED: u32 = 1;

/// Returns whether a field is annotated as required by either
/// `google.api.field_behavior = REQUIRED` or
/// `(buf.validate.field).required = true`.
pub fn field_is_required(options: Option<&FieldOptions>) -> CodegenResult<bool> {
    let Some(options) = options else {
        return Ok(false);
    };
    let unknown_fields: &UnknownFields = &options.__buffa_unknown_fields;

    Ok(has_required_field_behavior(unknown_fields)?
        || protovalidate_required(unknown_fields, FIELD_RULES_REQUIRED, "buf.validate.field")?)
}

/// Returns whether a oneof is annotated `(buf.validate.oneof).required = true`.
pub fn oneof_is_required(options: Option<&OneofOptions>) -> CodegenResult<bool> {
    let Some(options) = options else {
        return Ok(false);
    };

    protovalidate_required(
        &options.__buffa_unknown_fields,
        ONEOF_RULES_REQUIRED,
        "buf.validate.oneof",
    )
}

fn has_required_field_behavior(unknown_fields: &UnknownFields) -> CodegenResult<bool> {
    for unknown_field in unknown_fields.iter() {
        if unknown_field.number != FIELD_BEHAVIOR_EXTENSION_NUMBER {
            continue;
        }

        match &unknown_field.data {
            // Unpacked repeated enum: one varint per value.
            UnknownFieldData::Varint(value) => {
                if *value == FIELD_BEHAVIOR_REQUIRED {
                    return Ok(true);
                }
            }
            // Packed repeated enum: a length-delimited run of varints.
            UnknownFieldData::LengthDelimited(bytes) => {
                let mut bytes = bytes.as_slice();
                while bytes.has_remaining() {
                    let value = decode_varint(&mut bytes).kind_context(
                        CodegenErrKind::InvalidDescriptor,
                        "failed to decode packed google.api.field_behavior value",
                    )?;
                    if value == FIELD_BEHAVIOR_REQUIRED {
                        return Ok(true);
                    }
                }
            }
            _ => return Err(invalid_option("google.api.field_behavior must be an enum")),
        }
    }

    Ok(false)
}

/// Reads a boolean `required` field out of a Protovalidate rules message.
/// Repeated occurrences of the extension merge, so the last value wins.
fn protovalidate_required(
    unknown_fields: &UnknownFields,
    required_field_number: u32,
    extension_name: &str,
) -> CodegenResult<bool> {
    let mut required = false;

    for unknown_field in unknown_fields.iter() {
        if unknown_field.number != PROTOVALIDATE_EXTENSION_NUMBER {
            continue;
        }
        let UnknownFieldData::LengthDelimited(bytes) = &unknown_field.data else {
            return Err(invalid_option(format!(
                "{extension_name} must be a length-delimited message"
            )));
        };

        let mut bytes = bytes.as_slice();
        while bytes.has_remaining() {
            let tag = Tag::decode(&mut bytes).kind_context(
                CodegenErrKind::InvalidDescriptor,
                format!("failed to decode {extension_name} rule tag"),
            )?;

            if tag.field_number() == required_field_number && tag.wire_type() == WireType::Varint {
                let value = decode_varint(&mut bytes).kind_context(
                    CodegenErrKind::InvalidDescriptor,
                    format!("failed to decode {extension_name}.required"),
                )?;
                required = value != 0;
            } else {
                skip_field_depth(tag, &mut bytes, buffa::message::RECURSION_LIMIT).kind_context(
                    CodegenErrKind::InvalidDescriptor,
                    format!("failed to skip {extension_name} rule"),
                )?;
            }
        }
    }

    Ok(required)
}

fn invalid_option(context: impl Into<String>) -> UniError<CodegenErrKind> {
    UniError::from_kind_context(CodegenErrKind::InvalidDescriptor, context.into())
}

#[cfg(test)]
pub(crate) mod test_support {
    use buffa::encoding::{Tag, WireType};
    use buffa::{UnknownField, UnknownFieldData};
    use connectrpc_codegen::codegen::descriptor::{FieldOptions, OneofOptions};

    use super::{
        FIELD_BEHAVIOR_EXTENSION_NUMBER, FIELD_BEHAVIOR_REQUIRED, FIELD_RULES_REQUIRED,
        ONEOF_RULES_REQUIRED, PROTOVALIDATE_EXTENSION_NUMBER,
    };

    pub fn field_behavior_required(packed: bool) -> FieldOptions {
        let mut options = FieldOptions::default();
        let data = if packed {
            // OUTPUT_ONLY (3) then REQUIRED (2), packed.
            UnknownFieldData::LengthDelimited(vec![3, FIELD_BEHAVIOR_REQUIRED as u8])
        } else {
            UnknownFieldData::Varint(FIELD_BEHAVIOR_REQUIRED)
        };
        options.__buffa_unknown_fields.push(UnknownField {
            number: FIELD_BEHAVIOR_EXTENSION_NUMBER,
            data,
        });
        options
    }

    pub fn protovalidate_field_required() -> FieldOptions {
        let mut options = FieldOptions::default();
        options.__buffa_unknown_fields.push(UnknownField {
            number: PROTOVALIDATE_EXTENSION_NUMBER,
            data: UnknownFieldData::LengthDelimited(rules_with_required(FIELD_RULES_REQUIRED)),
        });
        options
    }

    pub fn protovalidate_oneof_required() -> OneofOptions {
        let mut options = OneofOptions::default();
        options.__buffa_unknown_fields.push(UnknownField {
            number: PROTOVALIDATE_EXTENSION_NUMBER,
            data: UnknownFieldData::LengthDelimited(rules_with_required(ONEOF_RULES_REQUIRED)),
        });
        options
    }

    fn rules_with_required(required_field: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        // An unrelated nested rule first (`string = { max_len: 5 }`, field 14),
        // so the parser has to skip over it.
        Tag::new(14, WireType::LengthDelimited).encode(&mut bytes);
        bytes.extend_from_slice(&[2, 0x10, 5]);
        Tag::new(required_field, WireType::Varint).encode(&mut bytes);
        bytes.push(1);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use connectrpc_codegen::codegen::descriptor::FieldOptions;

    use super::test_support::{
        field_behavior_required, protovalidate_field_required, protovalidate_oneof_required,
    };
    use super::{field_is_required, oneof_is_required};

    #[test]
    fn absent_options_are_not_required() {
        assert!(!field_is_required(None).unwrap());
        assert!(!field_is_required(Some(&FieldOptions::default())).unwrap());
        assert!(!oneof_is_required(None).unwrap());
    }

    #[test]
    fn reads_unpacked_and_packed_field_behavior() {
        assert!(field_is_required(Some(&field_behavior_required(false))).unwrap());
        assert!(field_is_required(Some(&field_behavior_required(true))).unwrap());
    }

    #[test]
    fn reads_protovalidate_field_and_oneof_required() {
        assert!(field_is_required(Some(&protovalidate_field_required())).unwrap());
        assert!(oneof_is_required(Some(&protovalidate_oneof_required())).unwrap());
    }
}
