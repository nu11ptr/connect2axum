# MCP Generator

`protoc-gen-connect2mcp` exposes unary ConnectRPC methods as MCP (Model
Context Protocol) tools that an AI agent can call. It generates one
`<service>_mcp` Rust module per service. The `connect2axum-mcp` runtime crate
serves the generated tools over MCP's Streamable HTTP transport as an
`axum::Router`.

See [`examples/mcp`](../examples/mcp) for a complete application.

## Buf Usage

```yaml
plugins:
  - local: protoc-gen-connect2mcp
    out: src/generated/connect2axum
    strategy: all
    opt:
      - buffa_module=crate::proto
      - connect_module=crate::connect
      - config=connect2mcp.yaml
```

Use `strategy: all` whenever a config file is set. Buf otherwise runs the
plugin once per proto directory, and a run that cannot see a config entry's
service treats the entry as a mistake (see [Config File](#config-file)).

## Plugin Options

Options are comma-separated `name=value` pairs.

| Option | Default | Purpose |
| --- | --- | --- |
| `buffa_module` | `crate::proto` | Rust module root of the Buffa generated messages. |
| `connect_module` | `crate::connect` | Rust module root of the Connect Rust service traits. |
| `runtime_module` | `::connect2axum` | Rust path to the `connect2axum` runtime. |
| `mcp_module` | `::connect2axum_mcp` | Rust path to the `connect2axum-mcp` runtime. |
| `value_suffix` | `__` | Suffix for generated local bindings, avoiding collisions with your names. |
| `config` | none | Path to the YAML config file. |

## Which RPCs Become Tools

Every unary RPC in the files being generated becomes a tool, whether or not it
has a `google.api.http` binding. MCP sends the whole request message as the
tool's arguments, so no HTTP mapping is needed.

- Streaming RPCs are skipped with a warning. An MCP tool call is one request
  and one result.
- An RPC whose request message is not a JSON object in ProtoJSON, such as
  `google.protobuf.StringValue`, is an error. Exclude it.
- A file with no tools produces no output file.

## Config File

```yaml
# Optional. When present, only these are exposed.
include:
  - billing.v1.InvoiceService

# Always removed, with or without `include`.
exclude:
  - billing.v1.AdminService

# Run only after the user confirms each call. See Confirmation below.
confirm:
  - billing.v1.InvoiceService.DeleteInvoice
  - billing.v1.PaymentService

# Optional tool name overrides.
names:
  auth.v1.AdminService: AuthAdmin                  # replaces the service part
  billing.v1.InvoiceService.GetInvoice: get_invoice # replaces the whole name
```

- Entries are fully-qualified services (`package.Service`) or methods
  (`package.Service.Method`). There are no wildcards.
- Without `include`, every RPC is exposed apart from those in `exclude`. With
  `include`, only the listed entries are, and `exclude` still removes from
  them. The difference shows when you add an RPC later: without `include` it
  becomes a tool automatically.
- An entry that matches nothing in the plugin run is an error, so a renamed
  RPC cannot silently become exposed again. Keep one config file per
  `buf.gen.yaml`, listing only that run's RPCs.
- Naming a streaming method in `include` is an error.
- A service in `confirm` covers all of its tools.

## Tool Names

- Default: `{Service}_{Method}` with the proto names unchanged, such as
  `GreeterService_SayHello`.
- Names must be 1 to 128 characters from `A-Z`, `a-z`, `0-9`, `_`, `-` and
  `.`; the generator checks this.
- Duplicate names within a plugin run are a generation error. Duplicates
  across runs, such as `AdminService_Reset` from two packages, are caught by
  `make_router` at startup. Resolve either with `names`.

## Input Schemas

Each tool's `inputSchema` is a JSON Schema describing the ProtoJSON form of
its request message. It is the only documentation the model sees, so proto
comments become `description` fields.

| Proto | JSON Schema |
| --- | --- |
| `string` | `{"type": "string"}` |
| `bool` | `{"type": "boolean"}` |
| 32-bit integers | `{"type": "integer", "format": "int32"}` or `"uint32"` |
| 64-bit integers | `{"type": "string", "format": "int64"}` or `"uint64"` |
| `float`, `double` | `{"type": "number", "format": "float"}` or `"double"` |
| `bytes` | `{"type": "string", "format": "byte"}` |
| `enum` | `{"type": "string", "enum": [value names]}` |
| message | an object schema, inlined |
| `repeated T` | `{"type": "array", "items": T}` |
| `map<K, V>` | `{"type": "object", "additionalProperties": V}` |
| `oneof` | ordinary properties; each member's description says "At most one of: ..." |
| `Timestamp` | `{"type": "string", "format": "date-time"}` |
| `Duration`, `FieldMask` | `{"type": "string"}` with a format description |
| `Struct`, `Value`, `ListValue` | object, any, array |
| wrapper types | the wrapped scalar's schema |
| `Empty` | an object with no properties |

- Nested messages are inlined. A message that refers to itself, directly or
  indirectly, goes under `$defs` and is referenced with `$ref`.
- `oneof` uses descriptions rather than the `oneOf` keyword, which some model
  APIs reject in tool schemas.
- Unknown properties are not forbidden, because Buffa ignores unknown JSON
  fields.

### Required Fields

proto3 has no `required`, so a schema lists a field as required only when it
carries one of these annotations:

- `(google.api.field_behavior) = REQUIRED`
- Protovalidate `(buf.validate.field).required = true`

A oneof with `(buf.validate.oneof).required = true` is described as "Exactly
one of: ...". These only describe the requirement to the model; your
service's own validation still enforces it.

## Confirmation

A tool listed under `confirm` runs only after the user approves the call. The
generated code marks it with `Tool::requires_confirmation()`, which you can
also call yourself on a hand-written `Tool`.

The server asks through MCP elicitation, using the stateless round trip
introduced in protocol `2026-07-28`:

1. The first `tools/call` returns an `input_required` result instead of
   calling the service. It holds a form elicitation whose message names the
   tool and shows its arguments, with no fields to fill in.
2. The client shows that message to the user, who accepts, declines or
   cancels.
3. The client repeats the call with the answer in `inputResponses`. On
   "accept" the service runs. On "decline" or "cancel" the result is an
   error with code `permission_denied`, so the model knows the user said no.

The model never sees this exchange; the client handles it. That is the point:
unlike `destructiveHint` and other tool annotations, which only suggest that
a client ask, the server itself refuses to run the call until the user
approves it, even in a client set to always allow the server's tools.

Clients that cannot take part get an error result with code
`failed_precondition` and the tool does not run. That covers clients on
protocol revisions before `2026-07-28`, which can only elicit over a session
this stateless server does not keep, and clients that do not declare form
elicitation. Check that the clients you care about support this before
relying on `confirm` for tools they need.

Confirmation guards against the model acting without the user's say-so. It
is not authorization: the answer comes from the client, which a malicious
client could fake. Keep permission checks in your service or auth layer.

## Results And Errors

| Situation | MCP response |
| --- | --- |
| The service returns a response | a result with the response as ProtoJSON in `structuredContent` |
| The service returns a Connect error | an error result (`isError: true`) carrying the Connect error JSON (`code`, `message`, `details`) |
| The arguments do not decode | an error result with code `invalid_argument` and the decoder's message |
| The tool name is unknown | JSON-RPC error `-32602`, "tool not found" |
| A `confirm` tool is called | see [Confirmation](#confirmation) |
| `arguments` is absent | treated as `{}` |

Error results let the model read the message and correct its arguments.
Connect response headers and trailers are dropped; MCP has no place for them.

## Serving The Tools

```rust
let mcp = connect2axum_mcp::make_router(
    connect2axum_mcp::McpConfig::new("billing", env!("CARGO_PKG_VERSION"))
        .allowed_hosts(["api.example.com"]),
    [
        mcp::invoice_service_mcp::make_tools(invoices.clone()),
        mcp::admin_service_mcp::make_tools(admin.clone()),
    ],
)?;
let app = axum::Router::new().merge(rest).merge(mcp);
```

- The endpoint is served at `/mcp`; change it with `McpConfig::path`.
- **Allowed hosts default to `localhost`, `127.0.0.1` and `::1`**, which
  protects local servers from DNS rebinding. A deployed server rejects every
  request with 403 until you call `allowed_hosts` or, behind a proxy that
  already checks hosts, `allow_any_host`.
- The server is stateless. Clients on every protocol revision from
  `2024-11-05` to `2026-07-28` are served; those with an `initialize`
  handshake complete it without being given a session.
- Request headers and axum request extensions reach the service through the
  Connect `RequestContext`, as they do for REST.

## Authentication

Authentication belongs in an HTTP layer in front of the MCP route, as for
REST:

1. **It must be an HTTP layer.** An `unauthenticated` error returned by a
   service arrives as an in-band tool error with HTTP 200, which starts no
   login flow, and `tools/list` never reaches a service at all.
2. **Accept the resource's audience.** With OAuth, clients request tokens for
   the `resource` URL published in the metadata document below. Your token
   validator must accept that audience.
3. **Keep discovery public.** The metadata route must sit outside the auth
   layer, and browser-based clients need it and the MCP route covered by your
   CORS policy.

Clients that send a static header, such as an API key, need nothing more.

### OAuth Discovery

Clients that log in through OAuth find the authorization server from the
server's Protected Resource Metadata (RFC 9728). `connect2axum-mcp` can serve
it; it never inspects tokens.

```rust
let resource = connect2axum_mcp::ProtectedResource::new("https://api.example.com/mcp")?
    .authorization_server("https://auth.example.com")
    .scopes(["mcp:tools"]);
let discovery = connect2axum_mcp::protected_resource_router(&resource)?;

// In your auth layer, on a missing or invalid token:
let challenge = connect2axum_mcp::bearer_challenge(&resource, None)?;
// respond 401 with `WWW-Authenticate: {challenge}`
```

The document is served at `/.well-known/oauth-protected-resource/mcp`, the
path-specific location clients try first. A plain 401 without the challenge
header also works, because clients fall back to that location, but the header
saves a round trip.

Your identity provider must support what MCP clients expect: authorization
server metadata discovery, PKCE, and a way for unknown clients to obtain a
client ID (Client ID Metadata Documents or dynamic client registration).
Without the last, each client has to be registered by hand.

## Not Supported

- Streaming RPCs as tools.
- Output schemas (`outputSchema`); results are still structured JSON.
- Tool annotations such as read-only hints. Use `confirm` for tools that
  should not run without the user's approval.
- Protovalidate rules other than `required`, such as length or range limits.
- stdio transport, MCP resources and prompts, and server-pushed messages.
