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
and generated `MessageView::decode_view` borrow from protobuf bytes. ConnectRPC's
own JSON request path confirms this: `decode_request_view` deserializes JSON into
the Buffa owned message, re-encodes that message as protobuf bytes, then decodes
an `OwnedView` from those bytes.

The response side has the same important boundary. Connect-generated service
traits can return owned messages, generated output views, `OwnedView<...>`, or
`MaybeBorrowed`. However, ConnectRPC intentionally supports view response bodies
only for protobuf output; `encode_view_body(..., CodecFormat::Json)` returns
`Unimplemented` because views do not implement `serde::Serialize`.

So the better REST adapter style is not a raw JSON-backed view. The better style
is "Connect JSON compatibility first, then remove the owned detour where we can":

- keep Connect service inputs as `OwnedView<...View<'static>>`;
- use Buffa-owned protobuf-JSON deserialization followed by
  `OwnedView::from_owned` as the compatibility baseline;
- add generated JSON-to-protobuf transcoders for supported request shapes so the
  hot path can become protobuf-JSON bytes to protobuf wire bytes to
  `OwnedView::decode`;
- for split REST requests, make generated body/query/path extraction use the
  same Buffa protobuf-JSON field rules as Buffa-generated owned messages, then
  progressively replace DTO construction with direct wire writing;
- allow service responses to be owned or view-shaped;
- provide a response wrapper for view bodies that can encode as protobuf for
  protobuf clients and encode JSON either directly from generated view JSON
  serializers or by falling back to protobuf-to-owned-to-JSON.

A composite request made of several `OwnedView`s is not the right abstraction:
Connect-generated service methods expect one concrete
`OwnedView<InputView<'static>>`, and Buffa's `OwnedView` owns one contiguous
`Bytes` buffer whose view borrows must all point into that buffer. A proxy would
require changing the Connect service trait shape or introducing unsafe lifetime
invariants across multiple backing buffers.

This should become its own review phase before OpenAPI, because it changes the
core adapter paradigm from "serde DTOs that happen to look like protobuf" to
"thin wrappers around Buffa's protobuf-JSON behavior."

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

## Phase 8: Efficient Buffa-Compatible REST Adapter Shape

Goal: make REST request/response handling match ConnectRPC's JSON behavior as
closely as possible, while reducing avoidable owned-message allocation on both
the request and response sides.

Implementation:

- Add a user-facing response utility in `crates/connect2axum` for services that
  want to return views without dropping JSON support:
  - expose a wrapper constructor such as `json_compatible_view(view)`,
  - implement `connectrpc::Encodable<M>` for the wrapper,
  - for `CodecFormat::Proto`, delegate to the view's protobuf encoding,
  - for `CodecFormat::Json`, first use a generated view JSON encoder when one
    exists,
  - otherwise encode the view as protobuf, decode `M`, then serialize `M`
    through Buffa's protobuf-JSON serde implementation.
- Generate an internal `JsonViewEncode<M>`-style trait implementation for
  output views where descriptor support is complete enough:
  - serialize scalar fields with protobuf JSON rules,
  - skip default fields the same way Buffa-owned JSON does,
  - handle enum names/numbers, bytes/base64, 64-bit integer strings, repeated
    fields, maps, nested messages, oneofs, and supported well-known types,
  - keep the implementation on a connect2axum-owned trait rather than
    implementing `serde::Serialize` for Buffa views directly, so it will not
    conflict if Buffa later adds view `Serialize`.
- Add runtime response helpers that keep the current fast path for owned
  responses:
  - try `body.encode(CodecFormat::Json)` first,
  - if that returns `ErrorCode::Unimplemented`, encode the body as protobuf
    with `CodecFormat::Proto`,
  - decode the protobuf bytes into the Buffa owned output message,
  - serialize that owned message with Buffa's serde/proto-JSON mapping,
  - preserve response headers, trailers, and error mapping.
- Tighten runtime bounds for REST JSON response helpers as needed:
  - `B: connectrpc::Encodable<M>`,
  - `M: buffa::Message + serde::Serialize`.
- Add generated-code tests proving a REST endpoint succeeds when the service
  returns:
  - an owned Buffa response,
  - an `OwnedView<OutputView<'static>>`,
  - a generated output view or `MaybeBorrowed` where the lifetime shape permits
    it.
- Keep runtime request helpers that mirror ConnectRPC's JSON request behavior
  without relying on hidden ConnectRPC codegen APIs:
  - deserialize JSON with `serde_json::from_slice::<InputOwned>`,
  - rely on Buffa-generated `serde` attributes/helpers for protobuf-compliant
    JSON behavior,
  - convert the owned message to `OwnedView<InputView<'static>>` with
    `OwnedView::from_owned`,
  - use this as the fallback for request shapes the direct transcoder does not
    support yet.
- Add generated JSON-to-protobuf request transcoders for supported request
  shapes:
  - parse protobuf-compliant JSON with `serde_json::Deserializer` visitors or
    seeds,
  - write protobuf tags and field values directly into a `BytesMut`/`Vec<u8>`
    using Buffa's public `encoding` and `types` helpers,
  - decode the final `Bytes` with `OwnedView::<InputView>::decode`,
  - avoid constructing Buffa owned messages, REST body DTOs, `String`, `Vec`, or
    `HashMap` values except where JSON parsing itself requires a temporary
    value, such as escaped strings, base64 bytes, nested length-delimited
    buffers, or maps.
- For generated REST request extraction, align all generated DTOs with Buffa's
  generated protobuf-JSON behavior:
  - use Buffa owned message types directly for `body: "*"` when no path/query
    fields are removed,
  - use Buffa owned sub-message types directly for `body: "message_field"`,
  - generate scalar body/query DTOs with Buffa-compatible serde attributes,
    including JSON names, proto-name aliases, enum handling, bytes handling,
    64-bit integer string handling, and wrapper/well-known-type behavior where
    Buffa supports it,
  - parse path parameters with the same scalar rules where HTTP path strings can
    sensibly map to protobuf JSON scalars.
- Prefer the direct transcoder for:
  - `body: "*"` whole-message JSON,
  - `body: "message_field"` sub-message JSON embedded into the parent request,
  - scalar path/query fields that can be parsed and encoded without intermediate
    DTO storage.
- Keep the Phase 6 owned reconstruction model as a compatibility fallback for:
  - unsupported well-known types,
  - complex maps/oneofs/extensions until implemented,
  - request shapes where direct transcoding would duplicate too much Buffa logic
    before tests cover it.
- Document the important limitation:
  - raw JSON bytes cannot safely back a Buffa view today,
  - efficient REST requests should become protobuf-JSON to protobuf wire to
    `OwnedView::decode`, not a proxy over multiple `OwnedView`s.
- Add benchmarks or at least allocation-counting tests around:
  - Buffa-owned compatibility path plus `OwnedView::from_owned`,
  - generated JSON-to-protobuf request transcoder plus `OwnedView::decode`,
  - owned response JSON direct path,
  - generated view-to-JSON response path,
  - view response protobuf-decode JSON fallback.

Review focus:

- The request adapter should now behave like Buffa/Connect JSON, not like
  default serde DTOs with renamed fields.
- The response adapter should allow Connect service authors to return views
  without breaking REST JSON endpoints.
- Direct wire construction is now an explicit efficiency goal, but it must be
  guarded by Buffa/Connect compatibility tests and fall back cleanly when a
  feature is not supported yet.

Phase gate:

```sh
buf lint
buf generate
git diff --exit-code
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 9: OpenAPI Generation

Goal: restore the old OpenAPI value, but generate it from descriptors instead
of `utoipa` derives on Prost structs.

Implementation:

- Add `openapi=true` plugin support.
- Generate a package/service OpenAPI function:
  - default name: `openapi`
  - return type: `utoipa::openapi::OpenApi`
  - no global side effects.
- Generate schemas from descriptor fields:
  - scalar fields,
  - enums,
  - nested messages,
  - repeated fields,
  - maps if Buffa/descriptor support is straightforward,
  - well-known types with documented first-pass mappings.
- Generate operation metadata:
  - method,
  - path,
  - tag,
  - path params,
  - query params,
  - request body,
  - response body,
  - comments as descriptions.
- Add plugin options for security:
  - `security=Bearer` for all generated operations,
  - defer per-service/per-method security until there is a concrete user need.
- Add Swagger UI to `examples/simple`.
- Add snapshot tests for generated OpenAPI JSON.

Review focus:

- Descriptor-derived OpenAPI should be independent of generated Rust DTOs.
- Keep first-pass schema support honest; unsupported proto features should
  produce clear plugin errors or documented generic schemas.

Phase gate:

```sh
buf lint
buf generate
git diff --exit-code
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Phase 10: Streaming Policy

Goal: make streaming behavior explicit and green without recreating the old
WebSocket subsystem by accident.

Implementation:

- Add streaming detection to the descriptor IR:
  - unary,
  - server streaming,
  - client streaming,
  - bidirectional streaming.
- Default policy:
  - generate REST handlers only for unary methods,
  - skip streaming methods with a clear codegen warning/comment,
  - rely on native Connect/gRPC-Web for streaming.
- Add an opt-in experimental server-streaming REST mode only if it stays small:
  - `streaming=server-ndjson`,
  - server-streaming response as NDJSON,
  - no client-streaming or bidirectional REST in v1.
- Do not port the old WebSocket helpers.
- Add `examples/streaming` that proves the Connect-generated service handles
  streaming without `connect2axum` REST involvement.
- Add tests for:
  - streaming methods are skipped by default,
  - unary methods in the same service still generate,
  - optional server-streaming NDJSON works if implemented,
  - WebSocket routes are not generated.

Review focus:

- This phase intentionally narrows scope. The old `ws-streaming` example should
  become a Connect-native streaming example, not a WebSocket replacement.

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
  - OpenAPI example.
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
- Connect Rust codegen library:
  <https://raw.githubusercontent.com/anthropics/connect-rust/main/connectrpc-codegen/src/lib.rs>
- Connect Rust plugin protocol re-exports:
  <https://raw.githubusercontent.com/anthropics/connect-rust/main/connectrpc-codegen/src/plugin.rs>
- Buffa README:
  <https://github.com/anthropics/buffa>
- Buf generate docs:
  <https://buf.build/docs/generate/>
- Buf `buf.gen.yaml` v2 docs:
  <https://buf.build/docs/configuration/v2/buf-gen-yaml/>
