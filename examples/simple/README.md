# connect2axum Simple Example

This example shows the intended greenfield workflow:

1. edit protobuf files in `proto/`,
2. run `buf generate`,
3. review the checked-in output in `src/generated/`,
4. implement the generated Connect service trait,
5. mount the generated REST router next to the native Connect router.

The generated code is checked in on purpose so reviewers can inspect exactly
what Buffa, Connect Rust, and connect2axum produce.

Required local plugins:

```sh
cargo install --locked protoc-gen-buffa protoc-gen-buffa-packaging connectrpc-codegen
go install github.com/grpc-ecosystem/grpc-gateway/v2/protoc-gen-openapiv3@main
cargo install --path crates/connect2axum-codegen # installs protoc-gen-connect2rest and protoc-gen-connect2openapi
```

From the workspace root:

```sh
buf lint
buf generate
cargo test -p simple --all-features
```

Run the server:

```sh
cargo run -p simple
```

REST endpoint:

```sh
curl -i \
  -X POST 'http://127.0.0.1:8000/v1/hello/Jane?salutation=Ahoy' \
  -H 'content-type: application/json' \
  --data '{"last_name":"Doe"}'
```

Connect protocol endpoint over HTTP:

```sh
curl -i \
  -X POST 'http://127.0.0.1:8000/hello.v1.GreeterService/SayHello' \
  -H 'content-type: application/json' \
  -H 'connect-protocol-version: 1' \
  --data '{"salutation":"Ahoy","firstName":"Jane","lastName":"Doe"}'
```

gRPC endpoint with `grpcurl`:

```sh
grpcurl \
  -plaintext \
  -import-path examples/simple/proto \
  -proto hello/v1/hello.proto \
  -d '{"salutation":"Ahoy","firstName":"Jane","lastName":"Doe"}' \
  127.0.0.1:8000 \
  hello.v1.GreeterService/SayHello
```

OpenAPI and Scalar:

```sh
curl -s http://127.0.0.1:8000/openapi.json | jq .
open http://127.0.0.1:8000/scalar
```
