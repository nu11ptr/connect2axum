# MCP Example

A small in-memory book catalog served three ways from one ConnectRPC service:
Connect, REST under `/v1`, and MCP (Model Context Protocol) at `/mcp`.

```sh
cargo run -p mcp
```

The server listens on `127.0.0.1:8003`. Point an MCP client that supports
Streamable HTTP at `http://localhost:8003/mcp`.

## What It Shows

[`proto/library/v1/library.proto`](proto/library/v1/library.proto) declares
four RPCs:

| RPC | REST | MCP tool |
| --- | --- | --- |
| `AddBook` | `POST /v1/books` | `LibraryService_AddBook` |
| `SearchBooks` | none | `LibraryService_SearchBooks` |
| `DeleteBook` | none | excluded in `connect2mcp.yaml` |
| `WatchBooks` (server streaming) | none | skipped: MCP tools are unary only |

The generated tool schemas exercise:

- required fields, from `(google.api.field_behavior) = REQUIRED`;
- an enum, listed by value name;
- a `map<string, string>`, as a JSON object;
- a nested message, inlined;
- a oneof, described as "At most one of: `author`, `genre`."

`Timestamp` and other well-known types are covered by the generator's unit
tests rather than here, because they need the `buffa-types` crate.

## Generated Code

[`buf.gen.yaml`](buf.gen.yaml) runs `protoc-gen-connect2mcp` with
`strategy: all`. Buf otherwise runs a plugin once per proto directory, and the
`google/api` run would reject `connect2mcp.yaml` entries for `library.v1`
because it cannot see them.

The output is
[`library.connect2mcp.rs`](src/generated/connect2axum/library/v1/library.connect2mcp.rs):
one `library_service_mcp` module with a handler per tool and a `make_tools`
function. [`src/lib.rs`](src/lib.rs) passes that to
`connect2axum_mcp::make_router` and merges the result into the app.

## Tests

`cargo test -p mcp` drives `tools/list` and `tools/call` through the full
router as raw JSON-RPC.

The example was also checked with a real MCP client, `rmcp` 3.11.0's
Streamable HTTP client, negotiating protocol revisions `2026-07-28`,
`2025-06-18` and `2024-11-05` against the running server. All three listed
both tools, added a book, and received a structured `invalid_argument` error
for empty arguments. The server is stateless, so older revisions complete
their `initialize` handshake without being given a session.
