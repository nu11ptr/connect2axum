# MCP Server Wrapper Implementation Plan

This plan adds a third wrapper over ConnectRPC services, next to REST and
WebSocket: a Model Context Protocol (MCP) server that exposes unary RPCs as
tools an AI agent can call. It records the design decisions already agreed and
splits the work into phases. Each phase is a review boundary and ends with a
green repository.

## What Gets Built

- `protoc-gen-connect2mcp`: a fifth plugin in `connect2axum-codegen`. It emits
  one `<service>_mcp` Rust module per service, with one handler function per
  exposed RPC and a `make_tools` function.
- `connect2axum-mcp`: a new runtime crate. It turns the generated tool sets
  into one `axum::Router` that speaks MCP over HTTP. It is the only crate that
  depends on the MCP SDK.
- A shared ProtoJSON JSON Schema builder in `connect2axum-codegen`. MCP needs
  it for tool input schemas, and it replaces the AsyncAPI generator's own
  schema code.
- `examples/mcp`: a runnable example with transport-level tests.

Nothing changes for users who do not run the new plugin or depend on the new
crate.

## Decisions

These were settled before the plan was written. Do not reopen them during
implementation without a concrete reason.

| Topic | Decision | Reason |
| --- | --- | --- |
| Runtime home | Separate crate `connect2axum-mcp`, like `connect2axum-scalar` | The MCP SDK shipped 1.0, 2.0 and 3.0 between March and July 2026. A separate crate keeps `connect2axum`'s version independent of that. |
| MCP SDK | `rmcp` only, with `default-features = false` and features `server` and `transport-streamable-http-server` | Official SDK. No macros are needed because the code is generated. |
| SDK isolation | `rmcp` is a private dependency. No `rmcp` type appears in generated code or in the public API of `connect2axum-mcp`. | An `rmcp` major bump must not force users to regenerate code or change their own code. |
| Exposure | Every unary RPC in the files being generated becomes a tool, whether or not it has a `google.api.http` binding | MCP sends the whole request message as arguments, so no HTTP mapping is needed. |
| Opt out | YAML config file with an `exclude` list; an `include` list switches to allowlist mode | Same `config=` pattern as the OpenAPI and AsyncAPI generators. No custom proto option. |
| Streaming RPCs | Skipped with a warning | An MCP tool call is one request and one result. WebSocket and AsyncAPI already cover streaming. |
| Tool names | `Service_Method`; a duplicate name is an error | Tool names are one flat list per server. Clashes are not expected, but must not pass silently. |
| Sessions | Stateless only | Horizontally scalable, and compatible with every client that speaks Streamable HTTP. |
| Allowed hosts | Runtime argument, not a plugin option | Host names differ per environment. |
| Required fields | Marked in the schema only when the proto says so | proto3 has no `required`. Two existing annotations are honored; see Phase 1. |
| Auth | "Discovery in, validation out" | The crate can publish OAuth discovery metadata. It never inspects a token; the application's own layer does that. |
| Output schemas | Deferred | Clients validate results against an output schema, so a schema bug would break a working tool. |

## Ground Rules

Phase gate, run at the end of every phase:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo install --path crates/connect2axum-codegen --locked --force
buf lint
buf generate                      # repository root, covers examples/simple
(cd examples/streaming && buf generate)
(cd examples/ws-streaming && buf generate)
(cd examples/mcp && buf generate) # from Phase 5 on
git diff --exit-code
```

Project conventions, unchanged from the earlier plans:

- Prefer `flexstr` owned string types over `String` where they fit.
- Use `uni_error` for fallible project code. No direct `anyhow` or `thiserror`.
- A crate new to the lockfile needs a strong case. This plan adds exactly one
  direct dependency, `rmcp`, and only to `connect2axum-mcp`. The codegen crate
  gains no dependencies.

Rules for generated code. The goal is output as easy to read as the REST and
WebSocket output, and robust against name clashes:

- Every path is absolute: `::connect2axum_mcp::…`, `::connect2axum::…`,
  `::connectrpc::…`, or rooted at the configured `buffa_module`,
  `connect_module`, `runtime_module` and `mcp_module`.
- Parameters and locals carry the configured `value_suffix` (`service__`,
  `ctx__`, `request__`), as in the REST output.
- The generator introduces no identifiers beyond the per-method handler
  functions and `make_tools`. Schemas and names are literals inside
  `make_tools`, not module-level constants.
- Handler function names, `make_tools` and module names go through
  `ensure_unique_generated_identifiers`, so an RPC named `MakeTools` fails at
  codegen with a clear message.

## Background

### MCP in brief

- A server publishes a list of tools (`tools/list`). Each tool has a name, a
  description and an `inputSchema`, which is a JSON Schema object.
- The model builds arguments from that schema alone, so the schema and
  descriptions are the API documentation.
- A client invokes a tool with `tools/call`, passing a name and a JSON object
  of arguments. The server returns one result.
- A failed tool run is reported inside a successful response with
  `isError: true`, so the model can read the message and correct itself. A
  JSON-RPC protocol error is reserved for requests the server cannot route.
- The HTTP transport is "Streamable HTTP": JSON-RPC over POST to one endpoint.
- Protocol revision `2026-07-28` removed the `initialize` handshake and
  sessions. Older revisions (`2025-03-26` to `2025-11-25`) have a handshake,
  and sessions were optional for the server.

### rmcp facts

Verified by reading the `rmcp` 3.11.0 source on 2026-11-18. The SDK releases
about weekly, so re-check these against the version actually pinned.

- `ServerHandler` is the trait to implement. The relevant methods are
  `get_info`, `list_tools`, `get_tool` and `call_tool`.
- `model::Tool` holds `input_schema: Arc<JsonObject>`, a raw JSON object.
  `Tool::new_with_raw` builds one without `schemars` types.
- `CallToolResult::structured(value)` and `::structured_error(value)` build
  results. Use them instead of assembling results by hand, so the wire shape
  follows the SDK across protocol revisions.
- `transport::streamable_http_server::StreamableHttpService` is a tower
  service. `new` takes a handler factory, a session manager and a
  `StreamableHttpServerConfig`.
- For stateless operation set `legacy_session_mode` to `false` and
  `json_response` to `true`, and use `session::never::NeverSessionManager`.
  The service then accepts POST only and answers GET and DELETE with 405.
- `StreamableHttpServerConfig` is `#[non_exhaustive]`. Build it from
  `Default::default()` and the `with_*` methods.
- `allowed_hosts` defaults to `localhost`, `127.0.0.1` and `::1`. Any other
  `Host` header gets a 403. A deployed server rejects every request until this
  is configured.
- The transport inserts the request's `http::request::Parts` into
  `RequestContext::extensions`. That is how HTTP headers and axum request
  extensions reach a tool handler.
- An unknown tool name is answered with `ErrorData::invalid_params("tool not
  found", None)` by the SDK's own router. Mirror that.
- The `server` feature forces `schemars` on, even though this crate supplies
  raw schemas. It cannot be turned off.

### Dependency cost of rmcp

Measured against the lockfile at commit `3840ea7`:

- License: `rmcp` is Apache-2.0. `schemars` is MIT. `unicode-normalization` is
  `(MIT OR Apache-2.0) AND Unicode-3.0`, the same expression `unicode-ident`
  already has in the lockfile. Everything else is MIT/Apache dual.
- Tree: 32 packages added to a crate that depends on `connect2axum`. Of those,
  20 names are new to the workspace, 9 are second versions of crates already
  locked (`syn` 3, `rand` 0.10, `digest` 0.11, `base64` 0.23, `getrandom` 0.4
  and their companions), and 3 are already locked at the same version.
- Compile time: a clean debug build went from 5.3s to 7.8s on an 18-core
  machine.
- MSRV: 1.88.

## Target Shape

### Generated code

For `hello/v1/hello.proto` the plugin writes `hello/v1/hello.connect2mcp.rs`.
This is the target shape; change it only for a reason found during
implementation.

```rust
// @generated by connect2mcp.
// This file is generated from hello/v1/hello.proto. Do not edit by hand.

/// Generated MCP tool handlers and tool set.
pub mod greeter_service_mcp {
    #![allow(unused_imports)]
    use std::sync::Arc;

    /// Sends a greeting.
    pub async fn say_hello<S>(
        service__: Arc<S>,
        ctx__: ::connectrpc::RequestContext,
        request__: crate::proto::hello::v1::HelloRequest,
    ) -> ::connect2axum_mcp::ToolResult
    where
        S: crate::connect::hello::v1::GreeterService + Send + Sync + 'static,
    {
        let request__ = match ::connect2axum::owned_view::<
            crate::proto::hello::v1::__buffa::view::HelloRequestView<'static>,
        >(&request__) {
            Ok(request__) => request__,
            Err(err) => return ::connect2axum_mcp::error_result(err),
        };
        let request__ = ::connectrpc::ServiceRequest::from_parts(
            request__.reborrow(),
            request__.bytes(),
        );
        ::connect2axum_mcp::tool_result::<
            crate::proto::hello::v1::HelloReply,
            _,
        >(service__.say_hello(ctx__, request__).await)
    }

    pub fn make_tools<S>(service: Arc<S>) -> ::connect2axum_mcp::Tools
    where
        S: crate::connect::hello::v1::GreeterService + Send + Sync + 'static,
    {
        ::connect2axum_mcp::Tools::builder(service)
            .tool(
                ::connect2axum_mcp::Tool::new(
                    "GreeterService_SayHello",
                    r#"{
  "type": "object",
  "properties": {
    "firstName": {
      "type": "string",
      "description": "The first name of the person to greet."
    }
  }
}"#,
                )
                .description("Sends a greeting."),
                say_hello::<S>,
            )
            .build()
    }
}
```

The handler mirrors the REST handler: the runtime decodes the JSON arguments
into the owned Buffa message, as axum's `Json<T>` does for REST, and the
generated code performs the same `owned_view` and `ServiceRequest::from_parts`
steps.

### Application wiring

```rust
let mcp = connect2axum_mcp::make_router(
    connect2axum_mcp::McpConfig::new("hello", env!("CARGO_PKG_VERSION"))
        .allowed_hosts(["api.example.com"]),
    [
        mcp::greeter_service_mcp::make_tools(greeter.clone()),
        mcp::admin_service_mcp::make_tools(admin.clone()),
    ],
)?; // Err on a duplicate tool name

let app = axum::Router::new().merge(rest).merge(mcp);
```

The returned router has one route, at `McpConfig::path` (default `/mcp`). The
application merges it wherever its auth layers apply.

### Plugin options

| Option | Default | Purpose |
| --- | --- | --- |
| `buffa_module` | `crate::proto` | As for REST and WS. |
| `connect_module` | `crate::connect` | As for REST and WS. |
| `runtime_module` | `::connect2axum` | As for REST and WS. |
| `mcp_module` | `::connect2axum_mcp` | Rust path to the MCP runtime crate. |
| `value_suffix` | `__` | As for REST and WS. |
| `config` | none | Path to the YAML config file. |

Unknown options are an error, as today. REST-only options such as
`streaming_content_type` are not accepted.

### Config file

```yaml
# Optional. When present, only these are exposed.
include:
  - billing.v1.InvoiceService

# Always removed, with or without `include`.
exclude:
  - billing.v1.InvoiceService.DeleteInvoice
  - billing.v1.AdminService

# Optional tool name overrides.
names:
  auth.v1.AdminService: AuthAdmin                  # replaces the service part
  billing.v1.InvoiceService.GetInvoice: get_invoice # replaces the whole name
```

- An entry is a fully-qualified service (`package.Service`) or method
  (`package.Service.Method`). There are no wildcards.
- With no config file, every unary RPC is exposed under its default name.
- Every entry must match something in the files the plugin run is generating.
  An unmatched entry is a codegen error, so a renamed RPC cannot silently
  become exposed again.
- A plugin run sees only the files Buf hands it. Keep one config file per
  `buf.gen.yaml`, and run the plugin with `strategy: all` when that
  `buf.gen.yaml` covers more than one proto directory. The unmatched-entry
  error message should say this.
- A streaming method named in `include` is an error. A streaming method that
  is only reached through a service-level entry, or through the default, is
  skipped with a warning on stderr.

### Tool names

- Default: `{Service}_{Method}` with the proto names unchanged, for example
  `GreeterService_SayHello`.
- MCP rule, enforced at codegen: 1 to 128 characters from `[A-Za-z0-9_.-]`.
- Duplicates within one plugin run are a codegen error.
- Duplicates across plugin runs cannot be seen at codegen. `make_router`
  returns an error at startup that names both tools.

### Input schema mapping

Property keys are the field's JSON name. Nested messages are inlined. A
message that refers to itself, directly or indirectly, goes under `$defs` and
is referenced with `$ref`.

| Proto | JSON Schema |
| --- | --- |
| `string` | `{"type":"string"}` |
| `bool` | `{"type":"boolean"}` |
| 32-bit integers | `{"type":"integer","format":"int32"}` or `"uint32"` |
| 64-bit integers | `{"type":"string","format":"int64"}` or `"uint64"`. Buffa also accepts JSON numbers. |
| `float`, `double` | `{"type":"number"}` with the existing `format` |
| `bytes` | `{"type":"string","format":"byte"}` |
| `enum` | `{"type":"string","enum":[…value names…]}` |
| message | `{"type":"object","properties":{…}}`, plus `required` when annotated |
| `repeated T` | `{"type":"array","items":T}` |
| `map<K, V>` | `{"type":"object","additionalProperties":V}` |
| `oneof` | Members stay ordinary optional properties. Each member's description gains "At most one of: `a`, `b`." or "Exactly one of: …" when the oneof is required. No `oneOf` keyword, because some model APIs reject combinators in tool schemas. |
| proto3 `optional` | Same as a plain field. Its synthetic oneof is ignored. |
| `Timestamp` | `{"type":"string","format":"date-time"}` |
| `Duration` | `{"type":"string"}` with a description such as `"3.5s"` |
| `FieldMask` | `{"type":"string"}` |
| `Struct` | `{"type":"object"}` |
| `Value` | `{}` |
| `ListValue` | `{"type":"array"}` |
| Wrapper types | The wrapped scalar's schema |
| `Any` | `{"type":"object","properties":{"@type":{"type":"string"}},"required":["@type"]}` |
| `Empty` | `{"type":"object","properties":{}}` |

Descriptions come from leading proto comments, through the existing
`comment_description`.

### Error mapping

| Situation | MCP response |
| --- | --- |
| Service returns `Ok` | `CallToolResult::structured` with the response as ProtoJSON |
| Service returns a `ConnectError` | `CallToolResult::structured_error` with the Connect error JSON (`code`, `message`, `details`) |
| Arguments fail to decode | `structured_error` with `code: "invalid_argument"` and the decoder's message |
| Unknown tool name | Protocol error `invalid_params("tool not found")` |
| `arguments` absent | Treated as `{}` |

Response headers and trailers from the Connect response are dropped. MCP has
no place for them.

## Phase 1: Descriptor IR Extensions

Goal: make the IR able to describe ProtoJSON accurately. No generator output
changes in this phase.

Implementation:

- Add enums to the IR: `EnumType { name, full_name, comments, values }` for
  top-level and nested enums, with `DescriptorIr::enum_type(full_name)`.
  Source-info paths per `descriptor.proto`: file `enum_type` is 5, message
  `enum_type` is 4, enum `value` is 2.
- Add `Message::map_entry: bool`, read from `MessageOptions.map_entry`.
- Add oneofs: `Message::oneofs: Vec<Oneof { name, required }>` and
  `Field::oneof: Option<usize>`. A field with `proto3_optional` set gets
  `Field::proto3_optional = true` and no oneof index.
- Add `Field::required: bool`, true when either annotation is present:
  - `google.api.field_behavior` containing `REQUIRED`: `FieldOptions`
    extension 1052, a repeated enum, value 2. Accept both packed and unpacked
    encodings. Confirm the numbers against `google/api/field_behavior.proto`.
  - Protovalidate `(buf.validate.field).required = true`: `FieldOptions`
    extension 1159 holding `FieldRules`, whose `required` is field 25.
- Set `Oneof::required` from `(buf.validate.oneof).required`: `OneofOptions`
  extension 1159 holding `OneofRules`, whose `required` is field 1.
- Read these extensions from `__buffa_unknown_fields`, the way
  `internal/http.rs` reads `google.api.http`. Put the parsing in a new
  `internal/field_options.rs`. No proto files need to be vendored for this.

Tests:

- Unit tests in `ir.rs` for each addition, built from descriptor fixtures like
  the existing ones: enum values, a `map` field, a real oneof, a proto3
  optional field, each of the three annotations.

Review focus:

- REST, WS, OpenAPI and AsyncAPI output is byte-identical. The phase gate's
  `git diff --exit-code` proves it.

## Phase 2: Shared Schema Builder

Goal: one ProtoJSON JSON Schema builder used by AsyncAPI now and MCP later.

Implementation:

- Add `internal/schema.rs` implementing the mapping table above over the IR.
- Give it two reference strategies:
  - components: every message becomes a named component and is referenced by
    `$ref`, using `PackagePrefixNamer` and `ComponentNameTracker`. This is
    what AsyncAPI needs.
  - inline: nested messages are inlined, and only recursive messages go under
    `$defs`. This is what MCP needs.
- Port `asyncapi.rs` to it and delete `SchemaRegistry`.
- Leave the OpenAPI path on grpc-gateway. Where `openapi/schema.rs` maps
  scalars for generated DTOs, call the shared scalar mapping so there is one
  copy.
- Verify first: how Buffa's JSON deserializer treats an unknown field. If it
  rejects them, emit `"additionalProperties": false` on message objects in the
  inline strategy so models do not invent fields. If it ignores them, emit
  nothing. Record the finding in a comment.

Tests:

- One golden test per row of the mapping table.
- Recursion (direct and mutual), cross-package messages, nested messages.
- Required fields and required oneofs.

Review focus:

- `examples/ws-streaming/src/generated/asyncapi/asyncapi.json` is expected to
  be unchanged, because that example uses only strings and repeated strings.
  Any diff must be explained by a row of the mapping table.
- AsyncAPI documents for protos with maps, well-known types, enums or oneofs
  will change. That is the intended correction. Note it for the release notes.

## Phase 3: Runtime Crate

Goal: `connect2axum-mcp` works end to end with hand-written tool handlers,
before any code is generated.

Implementation:

- Create `crates/connect2axum-mcp` and add it to the workspace. Add `rmcp` to
  `[workspace.dependencies]` with the features listed under Decisions. Set
  `rust-version = "1.88"` on the new crate.
- In `connect2axum`, make `encode_json_compatible` public so the new crate
  encodes responses with the same JSON-then-protobuf fallback as REST.
- Public API, all free of `rmcp` types:
  - `Tool::new(name, input_schema_json)` with `.description(…)`. Mark it
    `#[non_exhaustive]` and keep it builder-style so later additions do not
    break generated code.
  - `Tools::builder(service)`, `.tool(tool, handler)`, `.build()`. A handler
    is `Fn(Arc<S>, connectrpc::RequestContext, Req) -> impl Future<Output =
    ToolResult>` with `Req: DeserializeOwned`.
  - `ToolResult` (opaque), `tool_result::<M, B>(ServiceResult<B>)` and
    `error_result(ConnectError)`, following the error mapping table.
  - `McpConfig::new(name, version)` with builder methods for `title`,
    `instructions`, `path`, `allowed_hosts`, `allowed_origins` and
    `max_request_body_bytes`. Defaults match `rmcp`, including loopback-only
    hosts. Provide an explicit way to allow any host.
  - `make_router(config, tool_sets) -> McpResult<axum::Router>`. It parses
    each schema, rejects duplicate tool names, and builds the service.
  - Errors use `uni_error` with a small `McpErrKind`.
- Private `rmcp` wiring:
  - A cheap-to-clone server type implementing `ServerHandler`. `list_tools`
    returns every tool in one page.
  - `call_tool` reads `http::request::Parts` from the context extensions and
    builds the Connect context with `connect2axum::request_context(headers,
    extensions)`. With no `Parts` present it uses empty headers.
  - `StreamableHttpService` with the stateless settings listed under rmcp
    facts, mounted with `Router::route_service`.
- Verify first:
  - `StreamableHttpService` composes with `axum::Router::route_service`
    without an adapter.
  - Whether `rmcp` drops the handler future when the client disconnects. If
    not, race the handler against `RequestContext::ct`.

Tests, as raw JSON-RPC over `tower::ServiceExt::oneshot`, with no MCP client
dependency:

- Current protocol: `tools/list` and `tools/call` with headers
  `MCP-Protocol-Version: 2026-07-28`, `Mcp-Method`, `Mcp-Name` for calls, and
  the `_meta` keys `io.modelcontextprotocol/protocolVersion`, `clientInfo`
  and `clientCapabilities`.
- Older protocol: `initialize` with `protocolVersion: "2025-06-18"`,
  `notifications/initialized`, then `tools/list` and `tools/call`, with no
  session ID issued or required.
- Each row of the error mapping table.
- A header and a request extension set by an outer layer are visible in the
  handler's Connect context.
- A disallowed `Host` gets 403. GET gets 405.
- A duplicate tool name and an unparseable schema both fail `make_router`.

Review focus:

- `cargo doc` for the crate shows no `rmcp` type in any public signature.

## Phase 4: Tool Catalog

Goal: decide, inside the codegen crate, which RPCs become tools and what they
are called. No Rust is emitted yet.

Implementation:

- `McpOptions`: its own option parser, like `AsyncApiOptions`, accepting the
  options in the table above. It produces the `CodegenOptions` that
  `TypeResolver` needs.
- `McpConfig` YAML type with `include`, `exclude` and `names`, loaded the way
  `DocConfig` is.
- A catalog builder that returns, per service, the exposed methods with tool
  name, description and input schema. It applies:
  - unary only, with the streaming rules from the config section;
  - include and exclude;
  - default and overridden names, with charset and length checks;
  - duplicate detection through `guardrails`;
  - the unmatched-entry error;
  - a codegen error for a request message whose ProtoJSON form is not an
    object, such as `google.protobuf.StringValue`, telling the user to exclude
    that RPC.
- New `CodegenErrKind` variants for invalid config, unmatched config entry,
  invalid tool name and duplicate tool name.

Tests:

- Unit tests for every rule above, including a file with no HTTP bindings at
  all, which must still produce tools.

## Phase 5: Generator, Plugin Binary And Example

Goal: `buf generate` produces working MCP modules.

Implementation:

- `internal/mcp.rs`: emit the target shape from the catalog, using `quote`
  and `prettyplease` like `ws.rs`. Emit schemas as pretty-printed raw string
  literals, choosing a `#` count that cannot collide with the JSON content.
- Follow REST's handling of `google.protobuf.Empty` requests and responses.
- `generate_mcp` and `try_generate_mcp` in `lib.rs`, plus
  `src/bin/protoc-gen-connect2mcp.rs` and its `[[bin]]` entry.
- Add the new binary to the lists in `tests/plugin_decode.rs` and in the
  `generators_advertise_proto3_optional_support` test.
- `examples/mcp`, with its own `buf.yaml`, `buf.gen.yaml` and
  `connect2mcp.yaml`. Its proto should exercise: a nested message, an enum, a
  map, a oneof, a `Timestamp`, a required field, an RPC without an HTTP
  binding, an excluded RPC and a streaming RPC. Check the generated code in,
  like the other examples.

Tests:

- Generator unit tests on descriptor fixtures: multiple services in one file,
  cross-package messages, identifier collisions.
- Example tests through the full router: `tools/list` shows the expected
  names and schemas and omits the excluded and streaming RPCs; `tools/call`
  succeeds; a service error comes back as an error result; an RPC without an
  HTTP binding is callable.
- Manual check, recorded in the example README: connect one real MCP client
  to the running example and call a tool. Use a client that negotiates a
  pre-`2026-07-28` revision if one is available, since stateless operation
  with older clients was verified by reading `rmcp`, not by running one.

Review focus:

- Read the generated file as a user would. It should be as easy to follow as
  `hello.connect2rest.rs`.

## Phase 6: OAuth Discovery Helpers

Goal: let an application make its MCP endpoint usable by clients that log in
through OAuth, without the crate taking any part in token validation. All of
this is opt-in; an application that never calls these functions serves
nothing extra.

Background, per the `2026-07-28` authorization spec:

- The MCP server is an OAuth resource server. It must publish Protected
  Resource Metadata (RFC 9728) naming at least one authorization server.
- A client that gets a 401 reads the metadata URL from the `WWW-Authenticate`
  header if present. Otherwise it requests
  `/.well-known/oauth-protected-resource` followed by the MCP endpoint's path,
  and then the same path at the root. A plain 401 is therefore sufficient once
  the document is served.
- Validating the token, including that it was issued for this server, is the
  server's duty. Here that means the application's existing auth layer.

Implementation:

- `ProtectedResource::new(resource_url)` with `.authorization_server(url)`,
  `.scopes(…)` and `.resource_name(…)`.
- `protected_resource_router(&ProtectedResource) -> McpResult<axum::Router>`.
  It serves the RFC 9728 JSON at the well-known path derived from the resource
  URL's path. It returns an error when no authorization server is set. Parse
  the URL with `http::Uri`; do not add a URL crate.
- `bearer_challenge(&ProtectedResource, scope)` returning the
  `WWW-Authenticate` value for a 401, and `insufficient_scope_challenge(…)`
  for a 403. These are for the application's own auth layer to use.
- Nothing provider-specific, and no JWT code.

Tests:

- The document is served at the path-specific well-known URL with the
  required fields.
- In `examples/mcp`, a small bearer-token layer in front of the MCP route:
  an unauthenticated request gets 401 with the challenge header, and an
  authenticated call sees the identity the layer placed in the request
  extensions.

Docs must state three rules for integrators:

1. Authentication must be an HTTP layer in front of the MCP route. An
   `unauthenticated` error returned by a handler arrives as an in-band tool
   error with HTTP 200, which starts no login flow, and `tools/list` never
   reaches a handler at all.
2. The metadata document's `resource` value is what clients request tokens
   for. The application's validator must accept that audience.
3. The metadata route must be public, and browser-based clients need it and
   the MCP route covered by the application's CORS policy.

## Phase 7: Documentation And Release Wiring

Implementation:

- `docs/mcp-generator.md`, in the style of `docs/asyncapi-generator.md`:
  Buf usage, plugin options, config file, tool naming, schema mapping, error
  mapping, the allowed-hosts requirement and the three auth rules.
- `README.md`: the summary line, the installed plugin list, the plugin options
  table and a short MCP section.
- `.github/workflows/release-generators.yml`: add `protoc-gen-connect2mcp` to
  the `bins` list.
- Crate metadata for `connect2axum-mcp`. Version numbers and publishing are
  the maintainer's call; the crates touched are `connect2axum` (one function
  made public), `connect2axum-codegen` (new binary, corrected AsyncAPI
  schemas) and the new `connect2axum-mcp`.

## Deferred

Not part of this plan. Each is additive later.

- Streaming RPCs as tools.
- Output schemas (`outputSchema`), built with the same schema builder.
- Tool annotations such as read-only, derived from the method's
  `idempotency_level`.
- Mapping other Protovalidate rules (`max_len`, numeric bounds, `max_items`,
  `defined_only`) to JSON Schema keywords.
- A proto method option for exclusion.
- stdio transport, MCP resources and prompts.
- Stateful sessions and server-pushed messages such as "tool list changed".
- Token validation, per-tool scopes and per-caller tool lists.

## Source References

- `rmcp` 3.11.0: <https://crates.io/crates/rmcp/3.11.0>, source at
  <https://github.com/modelcontextprotocol/rust-sdk>
- MCP specification, revision 2026-07-28:
  <https://modelcontextprotocol.io/specification/2026-07-28>
- MCP authorization and discovery:
  <https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization>
  and
  <https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/authorization-server-discovery>
- RFC 9728, OAuth 2.0 Protected Resource Metadata:
  <https://datatracker.ietf.org/doc/html/rfc9728>
- In this repository: `crates/connect2axum-codegen/src/internal/ws.rs` for the
  generator pattern, `internal/http.rs` for extension parsing,
  `internal/asyncapi.rs` for the schema code being replaced, and
  `crates/connect2axum/src/lib.rs` for the runtime helpers being reused.
