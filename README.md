# connect2axum

Generate REST/OpenAPI endpoint wrappers over ConnectRPC services.

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

### Parsed But Reserved

These options are accepted by the parser but are not active behavior in the
current generators:

| Option | Default | Notes |
| --- | --- | --- |
| `openapi` | `false` | OpenAPI generation was deferred out of this project for now. |
| `service_state` | none | Parsed as `service.fqn:RustType`; current routers use `Arc<S>` state. |

### WebSocket Notes

`protoc-gen-connect2ws` only generates JSON WebSocket routes for streaming RPCs.
Unary RPCs stay REST/Connect-only. Client and bidirectional request streams end
with an empty text frame so the socket can remain open for any response frames.
