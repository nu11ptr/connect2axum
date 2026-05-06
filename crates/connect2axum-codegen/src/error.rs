use std::borrow::Cow;

use uni_error::{Cause, UniKind};

#[derive(Debug)]
pub enum CodegenErrKind {
    InvalidPluginOption,
    UnknownPluginOption,
    InvalidBooleanOption,
    InvalidServiceStateOption,
    FileToGenerateNotFound,
    InvalidDescriptor,
    InvalidHttpAnnotation,
    UnsupportedHttpRule,
    PathFieldNotFound,
}

impl UniKind for CodegenErrKind {
    fn context(&self, _cause: Option<Cause<'_>>) -> Option<Cow<'static, str>> {
        match self {
            Self::InvalidPluginOption => Some("invalid connect2axum plugin option".into()),
            Self::UnknownPluginOption => Some("unknown connect2axum plugin option".into()),
            Self::InvalidBooleanOption => Some("invalid boolean connect2axum plugin option".into()),
            Self::InvalidServiceStateOption => {
                Some("invalid connect2axum service_state option".into())
            }
            Self::FileToGenerateNotFound => {
                Some("file_to_generate was not found in the descriptor set".into())
            }
            Self::InvalidDescriptor => Some("invalid protobuf descriptor".into()),
            Self::InvalidHttpAnnotation => Some("invalid google.api.http annotation".into()),
            Self::UnsupportedHttpRule => Some("unsupported google.api.http rule".into()),
            Self::PathFieldNotFound => Some("google.api.http path field was not found".into()),
        }
    }
}

pub type CodegenResult<T> = uni_error::UniResult<T, CodegenErrKind>;
