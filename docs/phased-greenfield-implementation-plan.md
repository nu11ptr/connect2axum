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
- `../tonic2axum/tonic2axum/src`: current runtime helper behavior and the
  streaming/WebSocket code that should mostly be dropped.
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
because `protoc-gen-connect2axum` itself is a compiler plugin.

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
  - `openapi=true|false`
  - `runtime_module=::connect2axum`
  - `streaming_content_type=application/x-ndjson`
  - `value_suffix=__`
  - `type_suffix=__`
  - `body_message_suffix=Body`
  - `query_message_suffix=Query`
  - `service_state=package.Service:crate::MyService`
- Default options:
  - `buffa_module=crate::proto`
  - `connect_module=crate::connect`
  - `openapi=false`
  - `runtime_module=::connect2axum`
  - `streaming_content_type=application/x-ndjson`
  - `value_suffix=__`
  - `type_suffix=__`
  - `body_message_suffix=Body`
  - `query_message_suffix=Query`
  - no explicit `service_state`; generated routers are generic over
    `Arc<S>` where `S` implements the generated Connect service trait.
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
- Keep existing parsed plugin options stable for now so earlier phases do not
  churn, but treat `openapi=true` as inactive until the OpenAPI direction is
  revisited.
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

## Phase 11: Compatibility Hardening

Goal: cover the non-happy-path proto/codegen cases that usually make generator
crates painful to adopt.

Implementation:

- Add validation and tests for:
  - duplicate generated Rust identifiers,
  - duplicate routes,
  - unsupported HTTP custom verbs,
  - unsupported complex path templates,
  - missing `buffa_module`/`connect_module` coverage,
  - name collisions between generated DTOs and Buffa messages,
  - services with no REST bindings,
  - multiple services in one proto file,
  - same package split across multiple proto files,
  - cross-package inputs/outputs,
  - well-known types.
- Improve error messages so plugin failures point at:
  - proto file,
  - service,
  - method,
  - field when applicable.
- Add generated source formatting with `prettyplease`.
- Add a test utility that makes snapshot updates explicit and easy to review.

Review focus:

- This phase is about generator trustworthiness rather than new features.
- Fail early and loudly instead of generating code that fails mysteriously in a
  downstream crate.

Phase gate:

```sh
buf lint
buf generate
git diff --exit-code
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 12: Documentation And Release Readiness

Goal: turn the green implementation into a usable crate set.

Implementation:

- Update root README with:
  - crate purpose,
  - status,
  - quick start,
  - Buf install/generate workflow,
  - Axum composition example,
  - unary and streaming REST examples.
- Add API docs to public runtime helpers.
- Add `docs/migrating-from-tonic2axum.md`:
  - `build.rs` to Buf,
  - Tonic service impl to Connect service impl,
  - Prost messages to Buffa messages/views,
  - old WebSocket behavior to Connect-native streaming,
  - removed custom string work.
- Add `docs/plugin-options.md`.
- Add release checklist:
  - MSRV,
  - crate metadata,
  - license,
  - examples,
  - docs.rs feature set,
  - CI commands.
- Decide whether to publish:
  - one runtime crate plus one codegen crate,
  - or runtime crate only with codegen as an unpublished workspace helper.

Review focus:

- Docs should teach the new project style directly. Avoid presenting this as a
  thin rewrite of the Tonic-era builder.

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
- No generated WebSocket routes.
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
- Buf generate docs:
  <https://buf.build/docs/generate/>
- Buf `buf.gen.yaml` v2 docs:
  <https://buf.build/docs/configuration/v2/buf-gen-yaml/>
