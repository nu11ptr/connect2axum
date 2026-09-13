use std::io::Write as _;
use std::process::{Command, Stdio};

use buffa::Message as _;
use connect2axum_codegen::{CodeGeneratorRequest, CodeGeneratorResponse};
use connectrpc_codegen::codegen::descriptor::{
    DescriptorProto, FieldDescriptorProto, FileDescriptorProto,
    field_descriptor_proto::{Label, Type},
};

#[test]
fn plugins_decode_schemas_larger_than_the_runtime_element_budget() {
    let input = large_schema_request().encode_to_vec();

    // Descriptor structs are much larger than their wire encoding. A schema
    // spanning many ordinary files must use the compiler tooling budget.
    assert!(matches!(
        CodeGeneratorRequest::decode_from_slice(&input),
        Err(buffa::DecodeError::ElementMemoryLimitExceeded)
    ));
    buffa::DecodeOptions::new()
        .with_element_memory_limit(buffa_codegen::TOOLING_ELEMENT_MEMORY_LIMIT)
        .decode_from_slice::<CodeGeneratorRequest>(&input)
        .expect("the fixture fits the compiler tooling budget");

    for binary in [
        env!("CARGO_BIN_EXE_protoc-gen-connect2rest"),
        env!("CARGO_BIN_EXE_protoc-gen-connect2ws"),
        env!("CARGO_BIN_EXE_protoc-gen-connect2openapi"),
        env!("CARGO_BIN_EXE_protoc-gen-connect2asyncapi"),
    ] {
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start compiler plugin");
        child
            .stdin
            .take()
            .expect("piped plugin stdin")
            .write_all(&input)
            .expect("write compiler request");
        let output = child.wait_with_output().expect("wait for compiler plugin");
        assert!(
            output.status.success(),
            "{binary} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response = CodeGeneratorResponse::decode_from_slice(&output.stdout)
            .unwrap_or_else(|err| panic!("{binary} returned an invalid plugin response: {err}"));
        assert!(response.error.is_none(), "{binary}: {:?}", response.error);
        assert!(
            response.file.is_empty(),
            "{binary} generated unrequested files"
        );
    }
}

fn large_schema_request() -> CodeGeneratorRequest {
    const MESSAGES_PER_FILE: usize = 20;
    const FIELDS_PER_MESSAGE: usize = 30;
    let field_memory_per_file =
        MESSAGES_PER_FILE * FIELDS_PER_MESSAGE * size_of::<FieldDescriptorProto>();
    let file_count = buffa::DEFAULT_ELEMENT_MEMORY_LIMIT / field_memory_per_file + 1;

    CodeGeneratorRequest {
        proto_file: (0..file_count)
            .map(|file_index| FileDescriptorProto {
                name: Some(format!("schema_{file_index}.proto")),
                package: Some(format!("example.schema{file_index}")),
                syntax: Some("proto3".into()),
                message_type: (0..MESSAGES_PER_FILE)
                    .map(|message_index| DescriptorProto {
                        name: Some(format!("Record{message_index}")),
                        field: (1..=FIELDS_PER_MESSAGE)
                            .map(|number| FieldDescriptorProto {
                                name: Some(format!("attribute_{number}")),
                                number: Some(number as i32),
                                label: Some(Label::LABEL_OPTIONAL),
                                r#type: Some(Type::TYPE_STRING),
                                ..Default::default()
                            })
                            .collect(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            })
            .collect(),
        // No files are selected for output, so the OpenAPI plugin does not
        // need an external schema generator after decoding the request.
        ..Default::default()
    }
}
