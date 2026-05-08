use std::io::{Read as _, Write as _};

use buffa::Message as _;

fn main() {
    let mut input = Vec::new();
    std::io::stdin()
        .read_to_end(&mut input)
        .expect("failed to read protoc request from stdin");

    let request = connect2axum_codegen::CodeGeneratorRequest::decode_from_slice(&input)
        .expect("failed to decode protoc request");
    let response = connect2axum_codegen::generate_ws(&request);
    let output = response.encode_to_vec();

    std::io::stdout()
        .write_all(&output)
        .expect("failed to write protoc response to stdout");
}
