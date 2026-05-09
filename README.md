# connect2axum

Generate REST and/or Websockets endpoint wrappers over ConnectRPC services. Additionally, generate OpenAPI docs for these new endpoints.

## Plugin Options

Options are passed as comma-separated `name=value` pairs in `buf.gen.yaml`.

```yaml
plugins:
  - local: protoc-gen-connect2rest
    out: src/generated/connect2axum
    opt:
      - buffa_module=crate::proto
      - connect_module=crate::connect
  - local: protoc-gen-connect2ws
    out: src/generated/connect2axum
    opt:
      - buffa_module=crate::proto
      - connect_module=crate::connect
  - local: protoc-gen-connect2openapi
    out: src/generated/openapi
    strategy: all
    opt:
      - config=connect2openapi.yaml
```

### Active Options

| Option | Default | Used By | Purpose |
| --- | --- | --- | --- |
| `buffa_module` | `crate::proto` | REST, WS | Rust module root where Buffa generated messages are available. |
| `connect_module` | `crate::connect` | REST, WS | Rust module root where Connect Rust generated service traits are available. |
| `runtime_module` | `::connect2axum` | REST, WS | Rust path to the runtime helper crate/module. |
| `streaming_content_type` | `application/x-ndjson` | REST | REST streaming response content type. |
| `value_suffix` | `__` | REST, WS | Suffix for generated local bindings to avoid collisions with request fields. |
| `type_suffix` | `__` | REST | Suffix for generated DTO type names. |
| `body_message_suffix` | `Body` | REST | Suffix for generated body DTOs. |
| `query_message_suffix` | `Query` | REST | Suffix for generated query DTOs. |

### OpenAPI Generator

`protoc-gen-connect2openapi` wraps grpc-gateway's
`protoc-gen-openapiv3`, then patches the generated document for connect2axum
REST behavior. It supports `output`, `config`, `openapiv3_bin`,
`openapiv3_opt`, and `streaming_content_type` plugin options.

See [docs/openapi-generator.md](docs/openapi-generator.md) for config format
and backend details.

### WebSocket Notes

`protoc-gen-connect2ws` only generates JSON WebSocket routes for streaming RPCs.
Unary RPCs stay REST/Connect-only. Client and bidirectional request streams end
with an empty text frame so the socket can remain open for any response frames.
