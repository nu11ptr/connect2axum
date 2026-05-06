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
  - `service_state=package.Service=crate::MyService`
- Default options:
  - `buffa_module=crate::proto`
  - `connect_module=crate::connect`
  - `openapi=false`
  - `runtime_module=::connect2axum`
  - no explicit `service_state`; generated routers are generic over
    `Arc<S>` where `S` implements the generated Connect service trait.
- Reject unknown options with an error response instead of panic.
- Define the output contract:
  - one generated `*.connect2axum.rs` file per input proto that has at least one
    service with at least one `google.api.http` binding,
  - one package stitcher file per package when using `strategy: all`,
  - no output for files with no REST bindings.
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
  - JSON serialization helpers for Buffa-owned and Buffa-view responses.
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

## Phase 8: OpenAPI Generation

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

## Phase 9: Streaming Policy

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

## Phase 10: Compatibility Hardening

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

## Phase 11: Documentation And Release Readiness

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
