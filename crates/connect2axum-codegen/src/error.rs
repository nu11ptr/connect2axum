use std::borrow::Cow;

use uni_error::{Cause, UniKind};

#[derive(Debug)]
pub enum CodegenErrKind {
    InvalidPluginOption,
    UnknownPluginOption,
    OpenApiPluginFailed,
    OpenApiInvalidDocument,
    OpenApiMergeConflict,
    FileToGenerateNotFound,
    InvalidDescriptor,
    InvalidHttpAnnotation,
    UnsupportedHttpRule,
    PathFieldNotFound,
    RequestMessageNotFound,
    BodyFieldNotFound,
    TypeResolutionFailed,
    DuplicateGeneratedIdentifier,
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

pub type CodegenResult<T> = uni_error::UniResult<T, CodegenErrKind>;
