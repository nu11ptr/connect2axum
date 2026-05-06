# ConnectRPC + Buffa Pivot Plan

## Summary

This is a major rewrite from the old `tonic2axum` shape, not a dependency swap.
The old project was a `prost-build`/`tonic-prost-build` extension that appended
Axum REST, WebSocket, and OpenAPI code during `build.rs`. The new project should
be descriptor-first Buf codegen: `protoc-gen-buffa`,
`protoc-gen-connect-rust`, and a local plugin such as
`protoc-gen-connect2axum`.

Recommended direction:

- Drop Tonic entirely.
- Drop Prost entirely.
- Drop the custom string/type replacement work.
- Drop generated WebSocket endpoints for v1.
- Keep the core value as REST/OpenAPI transcoding over ConnectRPC service
  implementations, using Buffa-owned/view message types.

## Pivot Size By Section

| Section | Change Needed | Estimate | Notes |
|---|---:|---:|---|
| Workspace/build style | Very high | 80% | Replace example `build.rs` flow with checked-in `buf.yaml`/`buf.gen.yaml` and generated trees. |
| `tonic2axum-build` | Near rewrite | 90% | Replace `prost_build::ServiceGenerator` with a protoc/Buf plugin reading `CodeGeneratorRequest`. |
| HTTP annotations parser | Medium-high | 55% | Reuse concepts, but read `FileDescriptorProto`/extensions directly instead of `prost_reflect::DynamicMessage`. |
| Message/type model | High | 75% | Stop parsing generated Rust structs with `syn`; derive request/body/query structs from descriptors and Buffa type mapping. |
| Runtime crate | High | 70% | Remove `tonic::Request`, `tonic::Response`, and `tonic::Status`; add Connect context/error adapters. |
| REST handlers | High | 75% | Generated handlers call Connect service traits, likely by building Buffa owned requests and passing views. |
| OpenAPI | High | 70% | Prefer descriptor-derived OpenAPI generation over relying on `utoipa::ToSchema` derives on Buffa messages. |
| Streaming REST | High if kept | 70% | Keep out of v1 except optional server-streaming NDJSON later. |
| WebSockets | Delete/revisit | 95% | ConnectRPC already supports streaming and gRPC-Web; do not port WebSockets initially. |
| Custom strings/type replacement | Delete | 100% | Buffa views vs. owned messages make this unnecessary. |
| Examples/tests/docs | High | 75% | Regenerate fixtures around Buf output, Connect service implementations, and OpenAPI docs. |

## New Project Style

This project should not look like a `prost-build` plugin anymore. It should look
like a Buf/protoc plugin plus a small runtime helper crate.

Create a codegen binary crate, working name `connect2axum-codegen`, that exposes
`protoc-gen-connect2axum`. Keep a small runtime crate, working name
`connect2axum`, for Axum/Connect adapter helpers, shared error mapping, response
conversion, and optional streaming utilities.

Generated code should mirror the upstream Connect/Buffa style:

- `src/generated/buffa` from `protoc-gen-buffa`
- `src/generated/connect` from `protoc-gen-connect-rust`
- `src/generated/rest` from `protoc-gen-connect2axum`
- package `mod.rs` files assembled with `protoc-gen-buffa-packaging`

Use `buf.gen.yaml` as the public configuration surface. Run local plugins from
Buf, and use `strategy: all` for packaging and for `connect2axum` if route or
OpenAPI generation needs cross-file deduplication.

Plugin options should replace builder methods. Initial options:

- `connect_module=crate::generated::connect`
- `buffa_module=crate::generated::buffa`
- `openapi=true`
- `state_type=ServiceName=crate::MyService`
- `security=Bearer`

If per-service security remains important, prefer a small JSON/YAML plugin option
over recreating a fluent Rust builder API.

## Implementation Plan

1. Scaffold the new plugin/runtime crates without removing any old `tonic2axum`
   code until the new path compiles.
2. Implement descriptor-first route discovery from `google.api.http`, preserving
   the current simple path/body/query behavior first.
3. Generate unary REST handlers that:
   - use Axum extractors for path, query, and body,
   - construct Buffa owned request messages,
   - adapt to Connect generated trait inputs using Buffa views,
   - serialize successful responses through Buffa/Connect-compatible JSON
     helpers,
   - map Connect errors to HTTP status and body consistently.
4. Generate OpenAPI from descriptors rather than generated Rust derives. Emit a
   Rust function, such as `pub fn openapi() -> utoipa::openapi::OpenApi`, for
   parity with the old Swagger UI examples.
5. Convert examples to Buf:
   - remove `build.rs`,
   - add `buf.yaml` and `buf.gen.yaml`,
   - mount generated modules with stable `mod.rs` entrypoints,
   - implement Connect service traits instead of Tonic traits.
6. Delete or quarantine old-only work:
   - no `pbjson-build`,
   - no `prost-build` config,
   - no `tonic-prost-build`,
   - no custom string/type replacement roadmap in the active path,
   - no WebSocket port in v1.

## Test Plan

- Golden tests for plugin output from the existing `test/v1/test.proto` and
  `test_ws/v1/test_ws.proto` scenarios, updated for Buffa/Connect output.
- Compile tests for generated Buffa, Connect, and REST modules together.
- Integration tests for unary REST path/query/body extraction and error mapping.
- OpenAPI snapshot tests for routes, params, request bodies, response schemas,
  tags, and security.
- Example smoke tests using `buf generate`, `cargo check`, and one Axum request
  against the simple example.
- Explicit non-goal test: streaming RPCs are served by ConnectRPC, not generated
  WebSockets, in v1.

## Assumptions And Defaults

- This is a clean major-version/new-crate pivot, not a compatibility layer for
  existing `build.rs` users.
- WebSockets are dropped for the first pivot; native Connect/gRPC-Web handles the
  streaming use case.
- OpenAPI remains in scope, but generation should be descriptor-derived instead
  of relying on derives attached to generated message structs.
- Buffa's owned/view model replaces the custom string implementation work.
- The first implementation targets unary RPC transcoding. Streaming REST can be a
  later phase if Connect-native streaming is not enough for target users.

## Source References

- Connect Rust: <https://github.com/anthropics/connect-rust>
- Buffa: <https://github.com/anthropics/buffa>
- Buf generate docs: <https://buf.build/docs/generate/>
- Buf v2 `buf.gen.yaml` docs: <https://buf.build/docs/configuration/v2/buf-gen-yaml/>
