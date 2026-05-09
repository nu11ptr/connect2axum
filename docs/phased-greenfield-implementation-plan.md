# Phased Greenfield Implementation Plan

## Ground Rules

`tonic2axum` is the read-only reference implementation. All implementation
changes happen in `connect2axum`.

Reference paths:

- `../tonic2axum/tonic2axum-build/src/builder.rs`: current public builder
  surface and feature switches.
- `../tonic2axum/tonic2axum-build/src/http.rs`: current `google.api.http`
  parsing and path/body/query splitting behavior.
- `../tonic2axum/tonic2axum-build/src/message.rs`: current generated
  body/query DTO model.
- `../tonic2axum/tonic2axum-build/src/codegen/generator.rs`: current Axum
  handler/router/OpenAPI generation behavior.
- `../tonic2axum/tonic2axum/src`: current runtime helper behavior plus the
  HTTP/WebSocket streaming helpers to adapt selectively.
- `../tonic2axum/examples`: old user experience and scenarios to preserve or
  intentionally replace.

Every phase is a review boundary. The repo should be green at the end of each
phase:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

After Buf is introduced, phase gates also include:

```sh
buf lint
buf generate
git diff --exit-code
```

The runtime crate must not depend on Tonic or Prost. Codegen may depend on
`connectrpc-codegen`, `buffa-codegen`, and descriptor/plugin protocol types
because the `protoc-gen-connect2*` binaries are compiler plugins.

Project-wide Rust conventions:

- Prefer `flexstr` owned string types over `String` where they fit. Use `String`
  for generated source buffers, formatted construction, and external APIs that
  require it.
- Use `uni_error` for project fallible code. Do not add direct `anyhow` or
  `thiserror` dependencies unless there is a specific external integration that
  requires them.

## REST Adapter Shape Finding

Buffa views are protobuf-wire views, not JSON-document views. `OwnedView::decode`
and generated `MessageView::decode_view` borrow from protobuf bytes.
ConnectRPC's own JSON request path confirms this: `decode_request_view`
deserializes JSON into the Buffa owned message, re-encodes that message as
protobuf bytes with `OwnedView::from_owned`, then decodes an `OwnedView` from
those bytes. This is the compatibility baseline we should copy.

Buffa's ProtoJSON compatibility lives in generated owned structs plus
`buffa::json_helpers`, not in view types. Buffa-generated structs derive
`serde::Serialize` and `serde::Deserialize`, use `#[serde(default)]`, emit JSON
field names with proto-name aliases, and attach helpers such as
`proto_string`, `int64`, `uint64`, `float`, `double`, `bytes`, `proto_enum`,
`proto_seq`, and `proto_map`. Any connect2axum-generated request/body/query
structs must use the same helper modules and field attributes where possible,
or they will drift away from ConnectRPC JSON behavior.

The response side has the same important boundary. Connect-generated service
traits can return owned messages, generated output views, `OwnedView<...>`, or
`MaybeBorrowed`. However, ConnectRPC supports view response bodies only for
protobuf output today; JSON output requires an owned Buffa message because views
do not implement `serde::Serialize`.

So the better REST adapter style is "Connect JSON compatibility first, small
wrapper second":

- keep Connect service inputs as `OwnedView<...View<'static>>`;
- use Buffa-owned ProtoJSON deserialization followed by `OwnedView::from_owned`
  as the request baseline;
- use Buffa-generated owned message and enum serde implementations whenever the
  REST body can map directly to them;
- when REST path/query/body splitting requires connect2axum-generated structs,
  generate serde attributes that delegate to `buffa::json_helpers` instead of
  duplicating ProtoJSON parsing rules;
- keep path and query parsing explicitly scoped because ConnectRPC has no native
  path/query equivalent to compare against;
- allow service responses to be owned or view-shaped;
- provide a response wrapper for view bodies that can encode protobuf directly
  and encode JSON by converting through the Buffa owned message.

A composite request made of several `OwnedView`s is not the right abstraction:
Connect-generated service methods expect one concrete
`OwnedView<InputView<'static>>`, and Buffa's `OwnedView` owns one contiguous
`Bytes` buffer whose view borrows must all point into that buffer. A proxy would
require changing the Connect service trait shape or introducing unsafe lifetime
invariants across multiple backing buffers.

This should become its own review phase before OpenAPI, because it changes the
core adapter paradigm from "serde DTOs that happen to look like protobuf" to
"thin wrappers around Buffa's ProtoJSON behavior."

## Phase 1: Workspace And Quality Harness

Goal: create a minimal green Rust workspace in `connect2axum` with the final
crate layout, but no real codegen behavior yet.

Implementation:

- Create a root `Cargo.toml` workspace with shared package metadata, dependency
  versions, and lint settings.
- Add `crates/connect2axum` as the runtime library crate.
- Add `crates/connect2axum-codegen` as the codegen library crate and binary
  package. Its binary name should be `protoc-gen-connect2axum`.
- Keep the root `README.md` as the public summary and link to the docs.
- Add a tiny runtime API surface:
  - `connect2axum::VERSION` or equivalent harmless exported symbol.
  - No Axum/Connect helpers yet unless needed by compile tests.
- Add a no-op codegen API:
  - `connect2axum_codegen::generate(request) -> CodeGeneratorResponse`.
  - The binary reads a protoc plugin request from stdin and writes a valid empty
    response to stdout.
  - Use `connectrpc_codegen::plugin::{CodeGeneratorRequest, CodeGeneratorResponse}`
    so plugin protocol decoding stays aligned with Connect Rust/Buffa.
- Add baseline unit tests:
  - runtime crate compiles and exports the symbol.
  - codegen crate can decode an empty/default `CodeGeneratorRequest` and encode
    an empty `CodeGeneratorResponse`.

Review focus:

- Crate names, workspace layout, dependency policy, and green build harness.
- No user-facing codegen contract beyond "the plugin exists and is valid."

Phase gate:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 2: Plugin Options And Generated File Contract

Goal: make the plugin configurable and deterministic before adding semantic
proto parsing.

Implementation:

- Add a `CodegenOptions` type with strict parsing for comma-separated protoc
  options:
  - `buffa_module=crate::proto`
  - `connect_module=crate::connect`
  - `runtime_module=::connect2axum`
  - `streaming_content_type=application/x-ndjson`
  - `value_suffix=__`
  - `type_suffix=__`
  - `body_message_suffix=Body`
  - `query_message_suffix=Query`
- Default options:
  - `buffa_module=crate::proto`
  - `connect_module=crate::connect`
  - `runtime_module=::connect2axum`
  - `streaming_content_type=application/x-ndjson`
  - `value_suffix=__`
  - `type_suffix=__`
  - `body_message_suffix=Body`
  - `query_message_suffix=Query`
- Reject unknown options with an error response instead of panic.
- Define the Phase 2 placeholder output contract:
  - one generated `*.connect2axum.rs` file per `file_to_generate`,
  - no generated file when `file_to_generate` is empty,
  - actual filtering to files with `google.api.http` REST bindings starts in
    Phase 3 after descriptor/annotation parsing exists.
- Keep generated content as comments/placeholders in this phase, but make file
  names and package mapping final.
- Add tests for:
  - valid and invalid option parsing,
  - deterministic output file names,
  - empty input produces an empty response,
  - unknown option produces a protoc plugin error response.

Review focus:

- Public plugin options replace the old `Builder` fluent API.
- The generic service-state default intentionally replaces the old
  `Arc<dyn tonic_trait>` default, because Connect generated traits use plain
  async trait methods and are not intended as trait objects.

Phase gate:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 3: Descriptor IR And HTTP Annotation Extraction

Goal: port the valuable part of `tonic2axum-build/src/http.rs` into a
descriptor-first model that does not parse generated Rust.

Implementation:

- Add a codegen IR:
  - `ProtoFile`
  - `Service`
  - `Method`
  - `Message`
  - `Field`
  - `HttpBinding`
  - `HttpVerb`
- Parse `CodeGeneratorRequest.proto_file` into this IR using Buffa/Connect
  descriptor types.
- Extract `google.api.http` method options by extension number `72295728`.
- Support the old implementation's HTTP verbs first:
  - `get`
  - `post`
  - `put`
  - `delete`
  - `patch`
- Support the old implementation's body modes first:
  - no body,
  - `body: "*"`,
  - `body: "field_name"`.
- Preserve the old implementation's deliberately simple path-variable support:
  - `/path/{field}`
  - nested and custom path templates are parsed as unsupported and reported with
    clear plugin errors rather than silently generating wrong code.
- Parse source comments from `SourceCodeInfo` for services, methods, messages,
  and fields. This replaces the old `syn` doc-comment extraction.
- Add fixture protos copied from `tonic2axum-build/tests/proto` into
  `connect2axum` test fixtures, not imported from the old repo at test time.
- Add tests for:
  - unary POST with path fields and single-field body,
  - no binding means no generated route,
  - missing path field is an error,
  - unsupported path template is an error,
  - comments are attached to the expected IR nodes.

Review focus:

- The IR should describe protobuf/API intent, not Rust output.
- The extension parser is now the heart of the project; keep it small and
  heavily tested.

Phase gate:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 4: Request Decomposition And Buffa Type Resolution

Goal: reproduce the old path/body/query splitting semantics without depending
on Prost-generated structs.

Implementation:

- Add `RequestShape` planning:
  - `path_fields`
  - optional `query_shape`
  - optional `body_shape`
  - final request reconstruction plan.
- Port the current semantic behavior from `tonic2axum-build/src/http.rs`:
  - path variables remove fields from the request message,
  - `body: "*"` uses the original request when no fields were removed,
  - `body: "*"` creates a generated body DTO when fields were removed,
  - `body: "nested_message"` uses the existing nested message type,
  - `body: "scalar_field"` creates a one-field generated body DTO unless it is
    the entire request,
  - remaining fields become query DTOs.
- Add a `TypeResolver` for generated Rust paths:
  - owned Buffa message type path,
  - Buffa view type path,
  - Connect service trait path,
  - scalar field path/query Rust type.
- Base module-path rules on Connect Rust's split output style:
  - messages are mounted at `buffa_module`,
  - service traits are mounted at `connect_module`,
  - REST output is mounted separately.
- Explicitly model generated DTOs for REST-only body/query structs. These
  structs are generated by `connect2axum`, not by Buffa, and derive only what
  the handler/OpenAPI paths need.
- Add tests for:
  - same field partitioning as old `TestRequest`,
  - generated DTO name deduplication,
  - enum/path/query type resolution,
  - cross-package message references,
  - `google.protobuf.Empty`/empty request handling.

Review focus:

- This phase decides the new project's "style": descriptor-first request
  planning plus Buffa/Connect path resolution.
- There should be no `syn` parsing of generated message structs.

Phase gate:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 5: Rust Codegen Skeleton

Goal: generate real Rust modules that compile, route requests, and return
intentional placeholder responses.

Implementation:

- Generate one REST module per service, named with the same predictable style as
  old `greeter_axum`, but renamed for the new project:
  - default module suffix: `_rest`
  - default router function: `make_router`
- Generate:
  - REST-only body/query DTO structs,
  - one Axum handler per unary HTTP binding,
  - one `make_router` function per service.
- Handler signatures should be final:
  - `State(service): State<Arc<S>>`
  - `Path(...)`
  - `Query(...)`
  - `HeaderMap`
  - `Extensions`
  - optional `Json<BodyDto>`
- Router generics should be final:
  - `S: connect_module::package::ServiceTrait + Send + Sync + 'static`
  - `Arc<S>` state, cloned through Axum.
- Handler bodies return `501 Not Implemented` in this phase. This keeps the
  generated route surface compile-testable before Connect request construction
  is wired.
- Add a compile-test harness:
  - feed fixture descriptors into `connect2axum-codegen`,
  - write generated files into a temp crate,
  - provide tiny fake Buffa/Connect modules when possible,
  - compile with `cargo check` or `trybuild`.
- Add golden tests for generated source snapshots.

Review focus:

- Generated module shape, route naming, handler signatures, and generic state
  strategy.
- No runtime Connect call yet; this phase is about final Rust surface area.

Phase gate:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 6: Runtime Helpers And Unary Connect Calls

Goal: turn placeholder handlers into real unary REST-to-Connect calls.

Implementation:

- Add runtime helpers in `crates/connect2axum`:
  - request context construction from HTTP headers/extensions,
  - HTTP response construction from successful Connect responses,
  - Connect error to HTTP status/body mapping,
  - initial JSON serialization through ConnectRPC's `Encodable` contract for
    owned responses.
- Generate handler bodies that:
  - reconstruct the original Buffa owned request from path/query/body pieces,
  - convert that request into the generated Connect method's expected
    `OwnedView<...View<'static>>` input,
  - call `service.method(ctx, request).await`,
  - return JSON on success,
  - return mapped HTTP errors on `ConnectError`.
- Treat request metadata conservatively:
  - pass through HTTP headers that Connect context supports,
  - preserve `http::Extensions` when the Connect API exposes a supported path,
  - otherwise keep extensions available only to REST middleware and document the
    limitation.
- Add unit tests for runtime error mapping.
- Add generated-code integration tests for:
  - successful unary request,
  - service error response,
  - path + body + query reconstruction,
  - empty request,
  - body `"*"` request.

Review focus:

- This is the first real end-to-end behavior.
- The main risk is matching Connect Rust's exact `Context`/`RequestContext` API;
  keep any adapter code isolated in the runtime crate so API churn is contained.

Phase gate:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 7: Buf Workflow And Simple Example

Goal: prove the user-facing greenfield workflow: no `build.rs`, checked-in Buf
configuration, generated Buffa/Connect/REST code, and a running simple example.

Implementation:

- Add root or example-local `buf.yaml` and `buf.gen.yaml`.
- Configure plugins in the intended order:
  - `protoc-gen-buffa` to `src/generated/buffa` with views and JSON enabled,
  - `protoc-gen-buffa-packaging` for the Buffa tree with `strategy: all`,
  - `protoc-gen-connect-rust` to `src/generated/connect` with
    `buffa_module=crate::proto`,
  - `protoc-gen-buffa-packaging` for the Connect tree with `strategy: all` and
    `filter=services`,
  - `protoc-gen-connect2axum` to `src/generated/rest` with
    `buffa_module=crate::proto` and `connect_module=crate::connect`.
- Add `examples/simple` using the old `examples/simple/proto/hello/v1/hello.proto`
  scenario as the reference, copied into this repo.
- Remove any need for build scripts from the example.
- Check in generated example code only if that is the chosen repo policy. If
  generated code is not checked in, tests must run `buf generate` first and
  assert a clean diff from generated output.
- Implement the Connect service trait in the example.
- Compose an Axum router with:
  - native Connect fallback service,
  - generated REST router,
  - one basic health route.
- Add an integration smoke test that starts the example router in-process and
  calls the generated REST endpoint.

Review focus:

- This phase is about developer experience. A new user should understand the
  workflow by reading the example.

Phase gate:

```sh
buf lint
buf generate
git diff --exit-code
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 8: ConnectRPC-Aligned ProtoJSON REST Adapter

Goal: make REST JSON request/response handling use the same semantic source of
truth as ConnectRPC JSON: Buffa-generated owned serde implementations and
`OwnedView::from_owned`. Keep the adapter small and clear. Treat direct wire
construction as a later optimization, not the first implementation style.

Implementation:

- Add a source note in `crates/connect2axum` documenting the exact ConnectRPC
  request shape we mirror:
  - protobuf input: `OwnedView::<InputView>::decode(bytes)`,
  - JSON input: `serde_json::from_slice::<InputOwned>(bytes)`, then
    `OwnedView::<InputView>::from_owned(&owned)`.
- Add a runtime helper for the REST request body path:
  - deserialize JSON into the Buffa owned input or sub-message type,
  - map serde failures to the same kind of invalid-argument REST error we use
    elsewhere,
  - convert the final owned request to `OwnedView<InputView<'static>>` with the
    existing `owned_view` helper or a thin wrapper around
    `OwnedView::from_owned`.
- Update generated request code to prefer Buffa-owned types directly:
  - for `body: "*"` with no path/query overrides, deserialize the whole body
    into the Buffa owned input type;
  - for `body: "message_field"`, deserialize the body into that Buffa owned
    sub-message type and assign it into the parent owned request;
  - for scalar body fields, generate the smallest possible field wrapper that
    uses the same `buffa::json_helpers` module Buffa would use for that field.
- Align all connect2axum-generated request/body/query structs with Buffa's
  serde style:
  - emit `#[serde(default)]` where Buffa would;
  - use the JSON field name as `rename` and the proto field name as `alias`;
  - use `buffa::json_helpers` modules for strings, bools, numeric types, bytes,
    enums, repeated fields, maps, nullable/default behavior, and supported
    well-known types;
  - avoid local ProtoJSON parsers that Buffa does not model directly; Even query/path
    can use Buffa parsers because we generate them as new structs
  - keep these generated structs internal implementation details, not public
    user-facing DTOs.
- Keep path and query parsing deliberately scoped:
  - path/query values start as URL-decoded strings, while ConnectRPC JSON has
    only body JSON, so there is no native ConnectRPC behavior to copy exactly;
  - parse scalar path/query fields with generated helpers that feed Buffa-style
    field wrappers;
  - support repeated query fields where the descriptor shape is unambiguous;
  - reject complex message/map/oneof query fields with a clear generated error
    until we have compatibility tests for them.
- Add a user-facing response utility in `crates/connect2axum` for services that
  want to return views without dropping REST JSON support:
  - expose a wrapper constructor such as `json_compatible_view(view)`;
  - for protobuf output, delegate to the view's protobuf encoding;
  - for JSON output, convert through the Buffa owned output message and serialize
    that owned message with Buffa's serde/ProtoJSON mapping;
  - do not generate view JSON serializers in this phase.
- Keep the current owned response fast path:
  - if the response body can already encode JSON through ConnectRPC, use it;
  - only use the view-to-owned JSON fallback when the body reports that JSON is
    unsupported.
- Add ProtoJSON conformance fixtures before expanding supported field shapes:
  - create a proto fixture that covers JSON names and aliases, strings, bools,
    signed/unsigned integers, 64-bit integer strings, floats including
    `NaN`/`Infinity`, bytes, enums, nested messages, repeated fields, maps,
    optional/oneof fields, and supported well-known types;
  - for whole-body REST requests, compare REST behavior with the native
    ConnectRPC JSON endpoint for equivalent inputs;
  - for split path/query/body requests, test against the documented generated
    mapping because ConnectRPC has no path/query endpoint shape;
  - include rejection tests for unsupported complex path/query fields.
- Document the performance stance:
  - JSON compatibility and reuse of Buffa/ConnectRPC code comes first;
  - the expected request path is JSON to Buffa owned message to protobuf bytes
    to view;
  - this performs extra work compared with protobuf, but JSON parsing is already
    the expensive compatibility path;
  - direct JSON-to-protobuf transcoders and generated view JSON serializers are
    deferred until conformance tests prove the baseline and benchmarks show they
    are worth the extra generator complexity.

Review focus:

- REST body JSON should behave like ConnectRPC JSON because it uses the same
  Buffa-owned serde implementation.
- Any generated connect2axum structs should visibly delegate ProtoJSON behavior
  to `buffa::json_helpers`.
- Path/query support should be honest about where it extends beyond native
  ConnectRPC behavior.
- Response support should make view-returning services usable with REST JSON
  without inventing a second JSON encoder.

Phase gate:

```sh
buf lint
buf generate
git diff --exit-code
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 9: OpenAPI Generation Deferred

OpenAPI generation is intentionally skipped for now. The likely direction is a
separate OpenAPI effort, and possibly a separate project, rather than putting
OpenAPI generation into `connect2axum`.

Implementation:

- Do not add new OpenAPI generator behavior in this phase.
- Keep OpenAPI out of the REST/WS plugin options. OpenAPI should be revisited
  as a separate generator rather than an inline REST generator mode.
- Do not add Swagger UI to the examples.

Review focus:

- The project should remain focused on REST wrappers over ConnectRPC services.
- Avoid adding OpenAPI-specific dependencies or public APIs while the strategy
  is TBD.

## Phase 10: First-Class NDJSON Streaming REST

Goal: generate REST wrappers for every `google.api.http` method in the proto
file, regardless of whether the RPC is unary, server-streaming,
client-streaming, or bidirectional-streaming. Streaming REST is first-class and
uses NDJSON, not WebSockets.

Implementation:

- Extend the IR and request-shape planning to treat streaming as a route shape,
  not a reason to skip generation:
  - unary: one request, one response;
  - server streaming: one request, NDJSON response stream;
  - client streaming: NDJSON request stream, one response;
  - bidirectional streaming: NDJSON request stream, NDJSON response stream.
- Preserve the existing `streaming_content_type` plugin option:
  - default: `application/x-ndjson`;
  - use it as the response content type for server/bidi streaming;
  - use it as the expected/request body content type for client/bidi streaming;
  - keep it overridable through the existing comma-separated plugin options.
- Add runtime helpers for NDJSON request streams:
  - read newline-delimited JSON objects from the Axum request body;
  - deserialize each line with Buffa-owned serde/ProtoJSON rules;
  - convert each owned request item to `OwnedView<InputView<'static>>` with the
    same `OwnedView::from_owned` strategy as unary REST;
  - return a `connectrpc::ServiceStream<OwnedView<InputView<'static>>>`;
  - map malformed JSON lines to `ConnectError::invalid_argument`;
  - document and test blank-line behavior explicitly.
- Add runtime helpers for NDJSON response streams:
  - consume `connectrpc::ServiceResult<connectrpc::ServiceStream<B>>`;
  - serialize each successful stream item as one JSON line;
  - use the Phase 8 JSON-compatible response path per item so owned responses
    and view responses both work;
  - preserve response headers where HTTP permits it;
  - map stream item errors to a documented terminal NDJSON error line or a
    documented stream failure, then test that behavior.
- Generate handler bodies by RPC shape:
  - unary keeps the Phase 8 behavior;
  - server streaming reconstructs one request exactly like unary, calls the
    Connect service, then emits an NDJSON response;
  - client streaming builds a request stream from the NDJSON body, calls the
    Connect service, then emits a unary JSON response;
  - bidirectional streaming builds a request stream from the NDJSON body, calls
    the Connect service, then emits an NDJSON response.
- Keep request and response conversion aligned with ConnectRPC/Buffa:
  - request stream items use Buffa generated owned structs or generated
    Buffa-compatible DTOs, then `OwnedView::from_owned`;
  - response stream items use ConnectRPC `Encodable<M>` JSON first, then the
    view-to-owned fallback from Phase 8 when JSON is unimplemented;
  - do not write custom ProtoJSON encoders/decoders for streaming.
- Handle path/query parameters for streaming deliberately:
  - server-streaming methods can use path/query exactly like unary methods;
  - client/bidi streaming methods with path or query parameters fail codegen
    with a precise error, matching the old `tonic2axum` boundary;
  - client/bidi streaming methods require a streamable body, usually
    `body: "*"`;
  - if a client/bidi streaming binding has no streamable body or requires
    path/query reconstruction, fail codegen instead of generating surprising
    behavior.
- Do not port or generate WebSocket routes in this phase:
  - no `ws` runtime module;
  - no websocket feature flag;
  - no websocket routes beside the NDJSON REST routes;
  - if WebSockets return later, they should likely be a separate generator
    binary.
- Add `examples/streaming` adapted from `../tonic2axum/examples/streaming`:
  - use Buf generation instead of `build.rs`;
  - check in generated Buffa, Connect, and connect2axum REST files;
  - include README instructions for REST NDJSON with `curl`;
  - include README instructions for native Connect/gRPC streaming where the
    current Rust Connect tooling supports it.
- Add generated-code and runtime tests for:
  - all four RPC shapes generate handlers and routes;
  - NDJSON request streams decode multiple ProtoJSON lines;
  - NDJSON response streams emit one JSON object per line;
  - server-streaming methods support unary-style path/query/body extraction;
  - client/bidi streaming methods with path/query fail codegen with a precise
    error;
  - malformed NDJSON produces a deterministic error;
  - generated code contains no WebSocket routes or helpers.

Review focus:

- Streaming REST should feel like a natural extension of unary REST, not a
  bolted-on WebSocket replacement.
- The implementation should reuse Buffa/ConnectRPC request and response
  semantics per item.
- NDJSON behavior and error handling must be explicit enough for users to
  depend on.

Phase gate:

```sh
buf lint
buf generate
git diff --exit-code
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 11: REST Naming And Streaming Module Refactor

Goal: reshape the project so REST and WebSocket generation can grow side by
side without overloading the `connect2axum` name. This phase should not add
WebSocket behavior yet; it is the reviewable rename/refactor step that keeps
the current REST behavior green.

Implementation:

- Rename the current REST plugin binary:
  - from `protoc-gen-connect2axum`;
  - to `protoc-gen-connect2rest`.
- Rename generated REST file identity:
  - output file suffix changes from `*.connect2axum.rs` to
    `*.connect2rest.rs`;
  - generated comments say `@generated by connect2rest`;
  - generated REST modules and handler code continue to call the shared
    `connect2axum` runtime crate unless the runtime crate itself is renamed in
    a later explicit decision.
- Remove the old `protoc-gen-connect2axum` binary name entirely. This is
  greenfield, so there is no compatibility shim.
- Rename generated example folders and includes:
  - use one shared `src/generated/connect2axum` folder for all generated
    connect2axum files;
  - rely on file suffixes like `*.connect2rest.rs` and `*.connect2ws.rs` to
    avoid REST/WebSocket collisions;
  - update `examples/simple` and `examples/streaming` Buf configs, module
    includes, README snippets, and checked-in generated files.
- Split runtime streaming helpers out of `lib.rs`:
  - keep generic unary/runtime helpers in `lib.rs`, including
    `request_context`, `owned_view`, `json_owned_view`,
    `json_compatible_view`, `service_response`, `json_response`, and
    `error_response`;
  - add `crates/connect2axum/src/streaming/mod.rs`;
  - move NDJSON REST helpers into `crates/connect2axum/src/streaming/http.rs`;
  - re-export the HTTP streaming helpers from `lib.rs` so generated REST code
    either remains source-compatible or changes in a single obvious place.
- Refactor codegen internals for two generators:
  - keep shared descriptor IR, type resolver, shape planner, option parser, and
    formatting utilities in common modules;
  - move REST-specific generation into a `rest` module or similarly named
    boundary;
  - make binary entrypoints thin wrappers around shared `try_generate_*`
    functions.
- Preserve Phase 10 behavior exactly:
  - unary REST still works;
  - NDJSON server/client/bidi streaming REST still works;
  - no WebSocket routes are emitted in this phase.

Review focus:

- Naming should make the generated surfaces obvious:
  - `connect2rest` is the REST generator;
  - `connect2ws` will be the WebSocket generator;
  - `connect2axum` remains the shared Axum runtime crate.
- The refactor should reduce future branching, not create a parallel copy of
  the REST generator.
- Existing generated examples should remain easy to inspect.

Phase gate:

```sh
buf lint
buf generate
git diff --exit-code
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 12: JSON WebSocket Generator And Example

Goal: add optional WebSocket support as a separate generator,
`protoc-gen-connect2ws`, while continuing to reuse Buffa/ConnectRPC JSON
semantics and the shared `connect2axum` runtime. This should feel like the old
`tonic2axum` WebSocket support in shape, but JSON-only and Connect-native under
the hood.

Implementation:

- Add a new codegen binary:
  - binary name: `protoc-gen-connect2ws`;
  - generated file suffix: `*.connect2ws.rs`;
  - generated comment: `@generated by connect2ws`;
  - default output folder in examples: `src/generated/connect2axum`.
- Keep REST and WebSocket generation optional through Buf:
  - users run `protoc-gen-connect2rest` when they want HTTP/REST routes;
  - users run `protoc-gen-connect2ws` when they want WebSocket routes;
  - examples can run both into `generated/connect2axum` because generated file
    suffixes differ.
- Add `crates/connect2axum/src/streaming/ws.rs`:
  - `upgrade_to_ws` to split the upgraded socket and preserve request headers
    and extensions;
  - `make_ws_request` for server-streaming methods whose first text frame is a
    complete request JSON object;
  - `make_ws_stream_request` for client/bidi methods, converting each text
    frame into a Buffa-owned request and then an
    `OwnedView<InputView<'static>>`;
  - `process_ws_response` for client-streaming unary responses;
  - `process_ws_stream_response` for server/bidi streaming responses;
  - `close_ws` plus a Connect error to WebSocket close-frame mapping.
- Keep WebSocket payloads JSON-only:
  - accept text frames containing one complete ProtoJSON object per frame;
  - for client/bidi request streams, reserve an empty text frame as the
    end-of-request-stream marker because WebSocket has no half-close that still
    allows a unary response;
  - reject or close on binary frames instead of supporting protobuf frames;
  - do not generate `/ws/proto` routes;
  - default route suffix is `{http_path}/ws`.
- Reuse Connect/Buffa conversion logic:
  - request frames deserialize through Buffa-owned serde/ProtoJSON helpers;
  - requests enter service handlers as the same `OwnedView<...View<'static>>`
    type generated by Connect Rust;
  - response frames use ConnectRPC `Encodable<M>` with JSON first and the
    Phase 8 view-to-owned fallback when JSON is unimplemented;
  - do not write a custom ProtoJSON encoder/decoder for WebSockets.
- Generate routes only for streaming RPCs:
  - server streaming: read one request text frame, call the service, send each
    response item as a text JSON frame, then close normally;
  - client streaming: convert all incoming text frames into a
    `ServiceStream`, call the service, send the unary response as one text JSON
    frame, then close normally;
  - bidirectional streaming: convert incoming text frames into a
    `ServiceStream`, call the service, send response stream items as text JSON
    frames, then close normally;
  - unary RPCs remain REST/Connect only.
- Keep path/query behavior deliberately small:
  - client/bidi streaming methods with path or query parameters remain a
    codegen error, matching REST NDJSON;
  - server-streaming methods with path or query parameters remain valid for
    REST NDJSON but are skipped by `connect2ws`;
  - when `connect2ws` skips a server-streaming method for this reason, emit a
    warning log message naming the service, method, and skipped path/query
    binding;
  - WebSocket routes are derived from `google.api.http` paths;
  - request data comes from WebSocket JSON frames, not from path/query
    extraction;
  - frames contain the complete request message expected by the Connect service.
- Map errors predictably:
  - malformed request frame: close with an invalid/policy style code and a
    useful reason;
  - service error before any response item: close with a mapped Connect error
    code/reason;
  - stream item error: stop sending items and close with the mapped error;
  - normal completion: close with the normal close code.
- Adapt `../tonic2axum/examples/ws-streaming` into a new
  `examples/ws-streaming`:
  - use Buf generation instead of `build.rs`;
  - check in generated Buffa, Connect, `connect2rest`, and `connect2ws` code;
  - use standalone dependencies like the other examples;
  - README should show manual WebSocket JSON testing plus the REST/Connect
    equivalents where useful.
- Add tests for:
  - generated WebSocket handlers for server/client/bidi streaming methods;
  - no generated WebSocket handlers for unary methods;
  - JSON text frame request decoding into service views;
  - owned and view response bodies encoded as JSON text frames;
  - binary frames rejected deterministically;
  - malformed JSON closes deterministically;
  - `examples/ws-streaming` compiles and has integration coverage for the
    streaming WebSocket paths.

Review focus:

- The WebSocket generator should be small because it reuses the same request
  and response conversion policy as REST streaming.
- REST and WebSocket generation must remain independently opt-in.
- The runtime split should make `streaming/http.rs` and `streaming/ws.rs` read
  like siblings, not two unrelated adapters.

Phase gate:

```sh
buf lint
buf generate
git diff --exit-code
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 13: Generator Guardrails

Goal: keep the personal-tool version from failing mysteriously in generated
code, without turning the crate set into a polished public product.

Implementation:

- Keep generated source formatting through `prettyplease`.
- Fail during codegen for duplicate generated Rust identifiers:
  - service module names;
  - generated DTO names;
  - REST handler names;
  - WebSocket handler names.
- Fail during codegen for duplicate generated routes:
  - REST routes use the HTTP verb plus path;
  - WebSocket routes use the generated `{http_path}/ws` path.
- Add focused descriptor coverage for:
  - multiple services in one proto file;
  - cross-package input/output messages;
  - same-package messages split across multiple proto files;
  - `google.protobuf.Empty`;
  - server-streaming REST with path/query bindings remaining valid while
    WebSocket generation skips it.
- Add a short `docs/plugin-options.md`.

Skipped:

- Product polish, release readiness, migration docs, snapshot tooling, and
  broad compatibility hardening are intentionally out of scope unless this
  stops being a personal utility.

Phase gate:

```sh
buf lint
buf generate
git diff --exit-code
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 14: Shared API Document Core And OpenAPI Re-evaluation

Goal: reduce the OpenAPI generator from a large postprocessor into a small
adapter over a shared documentation core that can also feed AsyncAPI. This
phase explicitly re-evaluates whether wrapping grpc-gateway's
`protoc-gen-openapiv3` remains the right OpenAPI backend once AsyncAPI is in
scope.

Context:

- The current `protoc-gen-connect2openapi` wrapper is useful but large. It
  shells out to grpc-gateway, injects synthetic Go package names, merges output
  files, applies shared config, rewrites REST body DTO schemas, patches NDJSON
  streaming content types, and adds Connect-style errors.
- grpc-gateway's OpenAPI v3 generator is HTTP/OpenAPI-specific. It is an
  excellent reference for ProtoJSON schema decisions and comment harvesting, but
  its output is not a natural base for AsyncAPI because AsyncAPI describes
  channels, operations, messages, and protocol bindings rather than HTTP
  path-items.
- AsyncAPI does reuse familiar OpenAPI ideas: metadata, servers, security
  schemes, reusable components, tags, `$ref`, and JSON Schema-shaped payloads.
  That makes a shared core valuable, but not because OpenAPI can be
  mechanically transformed into AsyncAPI.

Implemented decision:

- Keep grpc-gateway's `protoc-gen-openapiv3` as the OpenAPI schema backend for
  now. Rewriting the full proto-to-OpenAPI path in Rust would duplicate a lot
  of already-working ProtoJSON/comment behavior before AsyncAPI has proven
  which pieces need to be shared.
- Put grpc-gateway behind `openapi::grpc_gateway` so the wrapper behavior is
  isolated and replaceable later.
- Use `oas3` as a typed OpenAPI v3.1 validation/navigation layer over the final
  merged document. It helps catch malformed output without forcing every
  extension-heavy patch through a typed builder API.
- Keep document assembly mostly `serde_json::Value` based, because the current
  work needs flexible OpenAPI extension handling and the future AsyncAPI
  generator will not be a mechanical transformation of OpenAPI paths.

Implementation:

- Split `crates/connect2axum-codegen/src/openapi/` into smaller modules:
  - `config`: shared YAML config for `info`, `servers`, security schemes,
    global security, headers, and content types;
  - `comments`: comment normalization for summaries/descriptions/tags;
  - `schema`: schema snippets for connect2axum generated REST DTOs;
  - `value`: common JSON object/array merge helpers;
  - `mod`: the `protoc-gen-connect2openapi` coordinator;
  - `document`: OpenAPI merge/config/body/streaming/error patching;
  - `grpc_gateway`: grpc-gateway delegation backend;
  - `model`: `oas3` validation and future typed OpenAPI navigation.
- Defer a full neutral operation/schema model until Phase 15. The current
  OpenAPI backend already supplies the bulk proto schema work, while AsyncAPI
  will need a WebSocket-oriented catalog that is not shaped like OpenAPI path
  items.
- Move the OpenAPI config behavior into the shared core:
  - `info`;
  - `servers`;
  - `securitySchemes`;
  - root `security`;
  - reusable header parameters;
  - Connect error response shape;
  - `streaming_content_type`.
- Preserve the existing native schema code only where connect2axum generates
  REST DTOs that grpc-gateway cannot see directly.
- Keep the equivalence burden low in this phase because grpc-gateway remains
  the schema backend. Add focused tests around merge conflicts, config/header
  patching, streaming content type patching, Connect error responses, and final
  `oas3` parsing.
- Evaluate but do not overcommit to model crates:
  - `oas3` is MIT licensed and targets OpenAPI v3.1.x parsing/navigation; use
    it for validation/navigation, not as the primary document builder.
  - `openapiv3` is MIT/Apache but targets OpenAPI v3.0.x, so it is not a good
    fit for a v3.1-first generator.
  - `asyncapi-rust-models` is MIT/Apache and models AsyncAPI 3.0; it is worth a
    spike, but AsyncAPI 3.1 is current, so using `serde_json::Value` plus small
    local structs may be less risky.
  - `asyncapi` is MIT/Apache but appears to model an older AsyncAPI shape
    without the 3.x `operations` object, so it should not be the primary target
    unless that changes.
  - Avoid compile-time/derive-first tooling such as `utoipa`-style generators;
    our source of truth is protobuf descriptors, not Rust handler code.
- Keep the simple example generated OpenAPI checked in.
- Add or update docs:
  - document `protoc-gen-connect2openapi` options;
  - document the OpenAPI config file;
  - record that OpenAPI remains delegated to grpc-gateway after the phase
    decision.

Review focus:

- The OpenAPI path should become smaller and easier to reason about, even if
  it still delegates schema generation to grpc-gateway internally.
- The shared schema/config/component code should be obviously reusable by
  AsyncAPI.
- We should not silently lose ProtoJSON-compatible schema behavior that
  grpc-gateway already handled correctly.
- The end state must be practical, not ideologically pure: native generation is
  only better if it is smaller and covers the supported connect2axum surface
  with confidence.

Phase gate:

```sh
buf lint
buf generate
git diff --exit-code
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 15: AsyncAPI Generator For WebSocket Routes

Goal: add `protoc-gen-connect2asyncapi` for generated JSON WebSocket routes,
using the shared API document core from Phase 14 rather than trying to derive
AsyncAPI from OpenAPI output.

Implementation:

- Add a new codegen binary:
  - binary name: `protoc-gen-connect2asyncapi`;
  - default output file: `asyncapi.json`;
  - plugin options mirror OpenAPI where practical:
    - `config=connect2api.yaml` or `config=connect2asyncapi.yaml`;
    - `output=asyncapi.json`;
    - `server_url=...` only if we decide to support simple inline config;
    - `streaming_content_type` is not relevant for WebSocket frames, but a
      `default_content_type=application/json` option is.
- Reuse Phase 14 shared inputs:
  - descriptor IR;
  - comment normalization;
  - protobuf schema registry;
  - service/method tags;
  - WebSocket route planner;
  - security config;
  - stable component naming.
- Generate AsyncAPI 3.x documents for the WebSocket generator's actual route
  behavior:
  - `asyncapi`;
  - `info`;
  - `servers` with `protocol: ws` or `wss`;
  - `defaultContentType: application/json`;
  - `channels` keyed by generated WebSocket route path;
  - `operations` keyed by stable service/method/action identifiers;
  - `components.messages` for input and output messages;
  - `components.schemas` for protobuf message payload schemas;
  - `components.securitySchemes` from shared config;
  - tags from service comments.
- Model WebSocket direction carefully:
  - server-streaming RPC: client sends one request message, server sends many
    response messages;
  - client-streaming RPC: client sends many request messages, server sends one
    response message after the empty-frame end marker;
  - bidi-streaming RPC: client sends many request messages and server sends
    many response messages;
  - unary RPCs are skipped because `connect2ws` does not generate unary
    WebSocket routes.

Implemented decision:

- `protoc-gen-connect2asyncapi` is implemented as a native descriptor/IR-based
  generator rather than as a postprocessor over OpenAPI.
- Channels are keyed by the generated WebSocket route path, such as
  `/hello/chat/ws`, with `request` and `response` channel messages.
- Operations are generated from the server application's point of view:
  `receive` for client-to-server JSON frames and `send` for server-to-client
  JSON frames.
- Reusable `components.messages` point at reusable `components.schemas`.
- Client and bidirectional streaming request operations document the empty text
  frame end-of-stream marker using `x-connect2axum-end-of-stream`.
- The `examples/ws-streaming` example now checks in generated AsyncAPI output
  under `src/generated/asyncapi/asyncapi.json`.
- Represent the end-of-client-stream marker explicitly:
  - add an `x-connect2axum-end-of-stream` extension documenting the empty text
    frame convention;
  - do not pretend this is part of the protobuf message schema.
- Preserve current WebSocket route support rules:
  - client/bidi methods with path or query bindings remain codegen errors in
    `connect2ws`;
  - server-streaming methods with path/query bindings are skipped by
    `connect2ws`, so they should also be skipped by `connect2asyncapi` with a
    warning;
  - AsyncAPI describes only routes generated by `connect2ws`.
- Add a generated AsyncAPI document to `examples/ws-streaming`, checked in next
  to the generated REST/WS code.
- Add tests for:
  - server/client/bidi WebSocket operations appear in AsyncAPI;
  - unary methods do not appear;
  - request and response messages reference shared schemas;
  - service/method comments appear as tags, summaries, and descriptions;
  - configured security schemes appear;
  - skipped server-streaming path/query routes do not appear;
  - generated `asyncapi.json` is valid JSON and stable across `buf generate`.
- Consider adding a Scalar-like docs route only if it is useful in practice.
  Scalar is primarily an OpenAPI viewer; AsyncAPI's own tooling may be a better
  fit later, and this phase should stay focused on producing the spec.

Review focus:

- AsyncAPI should document the WebSocket protocol we actually generate, not a
  theoretical websocketized REST API.
- The generator should be small because schema/config/comment machinery was
  already paid for in Phase 14.
- Any AsyncAPI limitations should be represented as explicit extensions or
  docs, not hidden assumptions.

Phase gate:

```sh
buf lint
buf generate
git diff --exit-code
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Explicit Non-Goals For V1

- No Tonic compatibility layer.
- No Prost runtime or generated message support.
- No `build.rs`-first API.
- No protobuf/binary WebSocket protocol support; WebSockets are JSON-only.
- No custom string/type replacement.
- No full grpc-gateway path-template implementation beyond the documented
  supported subset.
- No client generation; Connect Rust already owns that surface.

## Source References

- Connect Rust README:
  <https://github.com/anthropics/connect-rust>
- ConnectRPC request view decoding:
  `connectrpc::handler::decode_request_view` in `connectrpc` 0.4.
- Connect Rust codegen library:
  <https://raw.githubusercontent.com/anthropics/connect-rust/main/connectrpc-codegen/src/lib.rs>
- Connect Rust plugin protocol re-exports:
  <https://raw.githubusercontent.com/anthropics/connect-rust/main/connectrpc-codegen/src/plugin.rs>
- Buffa README:
  <https://github.com/anthropics/buffa>
- Buffa view ownership and JSON helpers:
  `buffa::view::OwnedView::{decode, from_owned}` and `buffa::json_helpers` in
  `buffa` 0.5.
- Tonic2Axum HTTP streaming reference:
  `../tonic2axum/tonic2axum/src/streaming/http.rs` and
  `../tonic2axum/examples/streaming`.
- Tonic2Axum WebSocket streaming reference:
  `../tonic2axum/tonic2axum/src/streaming/ws.rs`,
  `../tonic2axum/tonic2axum-build/src/codegen/generator.rs`, and
  `../tonic2axum/examples/ws-streaming`.
- Buf generate docs:
  <https://buf.build/docs/generate/>
- Buf `buf.gen.yaml` v2 docs:
  <https://buf.build/docs/configuration/v2/buf-gen-yaml/>
- gRPC-Gateway OpenAPI v3 generator docs:
  <https://grpc-ecosystem.github.io/grpc-gateway/docs/mapping/openapi_v3/>
- AsyncAPI v3.1 specification:
  <https://www.asyncapi.com/docs/reference/specification/v3.1.0>
- `oas3` Rust crate:
  <https://docs.rs/oas3>
- `asyncapi-rust-models` Rust crate:
  <https://docs.rs/asyncapi-rust-models>
