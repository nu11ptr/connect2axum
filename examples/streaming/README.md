# connect2axum Streaming Example

This example exercises REST wrappers for server streaming, client streaming,
and bidirectional streaming ConnectRPC methods. Generated code is checked in so
reviewers can inspect the Buffa, Connect Rust, and connect2axum output.

Required local plugins:

```sh
cargo install --locked protoc-gen-buffa protoc-gen-buffa-packaging connectrpc-codegen
cargo install --path crates/connect2axum-codegen # installs protoc-gen-connect2rest
```

From this example directory:

```sh
buf lint
buf generate
```

From the workspace root:

```sh
cargo test -p streaming --all-features
cargo run -p streaming
```

REST server-streaming endpoint:

```sh
curl -N -i \
  -X POST 'http://127.0.0.1:8001/v1/hello/expand' \
  -H 'content-type: application/json' \
  --data '{"firstName":"Jane","lastName":"Doe"}'
```

REST client-streaming endpoint:

```sh
printf '%s\n%s\n' \
  '{"firstName":"Jane","lastName":"Doe"}' \
  '{"firstName":"Ada","lastName":"Lovelace"}' |
curl -i \
  -X POST 'http://127.0.0.1:8001/v1/hello/collect' \
  -H 'content-type: application/x-ndjson' \
  --data-binary @-
```

REST bidirectional-streaming endpoint:

```sh
printf '%s\n%s\n' \
  '{"firstName":"Jane","lastName":"Doe"}' \
  '{"firstName":"Ada","lastName":"Lovelace"}' |
curl -N -i \
  -X POST 'http://127.0.0.1:8001/v1/hello/chat' \
  -H 'content-type: application/x-ndjson' \
  --data-binary @-
```

Connect protocol endpoint with `curl` uses Connect's streaming envelope, so the
response is binary-framed:

```sh
python3 - <<'PY' |
import json
import struct
import sys

msg = json.dumps({"firstName": "Jane", "lastName": "Doe"}, separators=(",", ":")).encode()
sys.stdout.buffer.write(b"\0" + struct.pack(">I", len(msg)) + msg)
PY
curl -N -s \
  -X POST 'http://127.0.0.1:8001/streaming.v1.GreeterService/Expand' \
  -H 'content-type: application/connect+json' \
  -H 'connect-protocol-version: 1' \
  --data-binary @- | xxd -g 1
```

gRPC endpoint with `grpcurl`:

```sh
grpcurl \
  -plaintext \
  -import-path examples/streaming/proto \
  -proto streaming/v1/streaming.proto \
  -d '{"firstName":"Jane","lastName":"Doe"}' \
  127.0.0.1:8001 \
  streaming.v1.GreeterService/Expand
```
