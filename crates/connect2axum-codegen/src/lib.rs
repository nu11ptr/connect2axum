//! Protoc/Buf code generation for `connect2axum`.
//!
//! Phase 1 provides a valid no-op plugin shell. Later phases will turn the
//! request descriptors into Axum REST routers and OpenAPI output.

pub use connectrpc_codegen::plugin::{CodeGeneratorRequest, CodeGeneratorResponse};

/// Generate a protoc plugin response for a request.
///
/// The Phase 1 implementation deliberately emits no files. This keeps the
/// compiler-plugin boundary real while later phases define the generated code
/// contract.
#[must_use]
pub fn generate(_request: &CodeGeneratorRequest) -> CodeGeneratorResponse {
    CodeGeneratorResponse::default()
}

#[cfg(test)]
mod tests {
    use buffa::Message as _;

    use super::{CodeGeneratorRequest, CodeGeneratorResponse, generate};

    #[test]
    fn empty_request_generates_empty_response() {
        let request = CodeGeneratorRequest::default();

        let response = generate(&request);

        assert!(response.file.is_empty());
        assert!(response.error.is_none());
    }

    #[test]
    fn plugin_protocol_messages_round_trip() {
        let request = CodeGeneratorRequest::default();
        let request_bytes = request.encode_to_vec();
        let decoded_request =
            CodeGeneratorRequest::decode_from_slice(&request_bytes).expect("request decodes");

        let response = generate(&decoded_request);
        let response_bytes = response.encode_to_vec();
        let decoded_response =
            CodeGeneratorResponse::decode_from_slice(&response_bytes).expect("response decodes");

        assert!(decoded_response.file.is_empty());
        assert!(decoded_response.error.is_none());
    }
}
