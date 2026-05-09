use std::borrow::Cow;

use uni_error::{Cause, UniKind};

/// Error categories produced by the connect2axum protoc generators.
#[derive(Clone, Copy, Debug)]
pub enum CodegenErrKind {
    /// A known plugin option was present but had an invalid value.
    InvalidPluginOption,
    /// The plugin parameter string contained an unsupported option name.
    UnknownPluginOption,
    /// The wrapped grpc-gateway OpenAPI generator failed.
    OpenApiPluginFailed,
    /// The OpenAPI document was not valid JSON or failed structural validation.
    OpenApiInvalidDocument,
    /// Multiple OpenAPI documents could not be merged safely.
    OpenApiMergeConflict,
    /// The generated AsyncAPI document failed structural validation.
    AsyncApiInvalidDocument,
    /// A requested `file_to_generate` was missing from the descriptor set.
    FileToGenerateNotFound,
    /// A protobuf descriptor was malformed or incomplete.
    InvalidDescriptor,
    /// A `google.api.http` annotation could not be decoded.
    InvalidHttpAnnotation,
    /// A `google.api.http` rule used an unsupported binding shape.
    UnsupportedHttpRule,
    /// A field referenced by a path template was missing from the request message.
    PathFieldNotFound,
    /// A method request message could not be resolved.
    RequestMessageNotFound,
    /// A field referenced by an HTTP body option was missing from the request message.
    BodyFieldNotFound,
    /// A protobuf type could not be mapped to the generated Rust type path.
    TypeResolutionFailed,
    /// Generated Rust identifiers would collide.
    DuplicateGeneratedIdentifier,
    /// Two generated handlers would register the same HTTP route.
    DuplicateRoute,
}

impl UniKind for CodegenErrKind {
    fn context(&self, _cause: Option<Cause<'_>>) -> Option<Cow<'static, str>> {
        match self {
            Self::InvalidPluginOption => Some("invalid connect2axum plugin option".into()),
            Self::UnknownPluginOption => Some("unknown connect2axum plugin option".into()),
            Self::OpenApiPluginFailed => Some("OpenAPI generator failed".into()),
            Self::OpenApiInvalidDocument => Some("invalid OpenAPI document".into()),
            Self::OpenApiMergeConflict => Some("OpenAPI documents could not be merged".into()),
            Self::AsyncApiInvalidDocument => Some("invalid AsyncAPI document".into()),
            Self::FileToGenerateNotFound => {
                Some("file_to_generate was not found in the descriptor set".into())
            }
            Self::InvalidDescriptor => Some("invalid protobuf descriptor".into()),
            Self::InvalidHttpAnnotation => Some("invalid google.api.http annotation".into()),
            Self::UnsupportedHttpRule => Some("unsupported google.api.http rule".into()),
            Self::PathFieldNotFound => Some("google.api.http path field was not found".into()),
            Self::RequestMessageNotFound => Some("request message was not found".into()),
            Self::BodyFieldNotFound => Some("google.api.http body field was not found".into()),
            Self::TypeResolutionFailed => Some("protobuf type could not be resolved".into()),
            Self::DuplicateGeneratedIdentifier => {
                Some("duplicate generated Rust identifier".into())
            }
            Self::DuplicateRoute => Some("duplicate generated route".into()),
        }
    }
}

/// Result type used by connect2axum code generation entry points.
pub type CodegenResult<T> = uni_error::UniResult<T, CodegenErrKind>;
