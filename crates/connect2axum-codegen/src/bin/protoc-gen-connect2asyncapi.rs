//! `protoc-gen-connect2asyncapi` compiler plugin entrypoint.

use std::io::{self, Read as _, Write as _};

use buffa::Message as _;
use connect2axum_codegen::CodeGeneratorRequest;
use uni_error::{ResultContext as _, SimpleResult};

fn main() -> SimpleResult<()> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .kind_default_context("failed to read CodeGeneratorRequest from stdin")?;

    let request = CodeGeneratorRequest::decode_from_slice(&input)
        .kind_default_context("failed to decode CodeGeneratorRequest")?;

    let response = connect2axum_codegen::generate_asyncapi(&request);
    let output = response.encode_to_vec();

    io::stdout()
        .write_all(&output)
        .kind_default_context("failed to write CodeGeneratorResponse to stdout")?;

    Ok(())
}
