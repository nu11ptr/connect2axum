///Shorthand for `OwnedView<HelloRequestView<'static>>`.
pub type OwnedHelloRequestView = ::buffa::view::OwnedView<
    crate::proto::streaming::v1::__buffa::view::HelloRequestView<'static>,
>;
///Shorthand for `OwnedView<HelloReplyView<'static>>`.
pub type OwnedHelloReplyView = ::buffa::view::OwnedView<
    crate::proto::streaming::v1::__buffa::view::HelloReplyView<'static>,
>;
///Shorthand for `OwnedView<HelloSummaryView<'static>>`.
pub type OwnedHelloSummaryView = ::buffa::view::OwnedView<
    crate::proto::streaming::v1::__buffa::view::HelloSummaryView<'static>,
>;
impl ::connectrpc::Encodable<crate::proto::streaming::v1::HelloReply>
for crate::proto::streaming::v1::__buffa::view::HelloReplyView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<crate::proto::streaming::v1::HelloReply>
for ::buffa::view::OwnedView<
    crate::proto::streaming::v1::__buffa::view::HelloReplyView<'static>,
> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(&**self, codec)
    }
}
impl ::connectrpc::Encodable<crate::proto::streaming::v1::HelloSummary>
for crate::proto::streaming::v1::__buffa::view::HelloSummaryView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<crate::proto::streaming::v1::HelloSummary>
for ::buffa::view::OwnedView<
    crate::proto::streaming::v1::__buffa::view::HelloSummaryView<'static>,
> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(&**self, codec)
    }
}
/// Full service name for this service.
pub const GREETER_SERVICE_SERVICE_NAME: &str = "streaming.v1.GreeterService";
/// Static [`Spec`](::connectrpc::Spec) for the server-side `Expand` RPC.
///
/// The dispatcher surfaces this on
/// [`RequestContext::spec`](::connectrpc::RequestContext::spec).
pub const GREETER_SERVICE_EXPAND_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/streaming.v1.GreeterService/Expand",
        ::connectrpc::StreamType::ServerStream,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the server-side `Collect` RPC.
///
/// The dispatcher surfaces this on
/// [`RequestContext::spec`](::connectrpc::RequestContext::spec).
pub const GREETER_SERVICE_COLLECT_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/streaming.v1.GreeterService/Collect",
        ::connectrpc::StreamType::ClientStream,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the server-side `Chat` RPC.
///
/// The dispatcher surfaces this on
/// [`RequestContext::spec`](::connectrpc::RequestContext::spec).
pub const GREETER_SERVICE_CHAT_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/streaming.v1.GreeterService/Chat",
        ::connectrpc::StreamType::BidiStream,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the server-side `Unary` RPC.
///
/// The dispatcher surfaces this on
/// [`RequestContext::spec`](::connectrpc::RequestContext::spec).
pub const GREETER_SERVICE_UNARY_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/streaming.v1.GreeterService/Unary",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Server trait for GreeterService.
///
/// # Implementing handlers
///
/// Handlers receive requests as `OwnedFooView` (an alias for
/// `OwnedView<FooView<'static>>`), which gives zero-copy borrowed access
/// to fields (e.g. `request.name` is a `&str` into the decoded buffer).
/// The view can be held across `.await` points. When two RPC types in
/// the same package would alias to the same `Owned<…>View` name (e.g.
/// a local message plus an imported one with the same short name), the
/// alias is suppressed for both and the request type is spelled as
/// `OwnedView<…View<'static>>` directly in the trait signature.
///
/// Implement methods with plain `async fn`; the returned future satisfies
/// the `Send` bound automatically. See the
/// [buffa user guide](https://github.com/anthropics/buffa/blob/main/docs/guide.md#ownedview-in-async-trait-implementations)
/// for zero-copy access patterns and when `to_owned_message()` is needed.
///
/// The `impl Encodable<Out>` return bound accepts the owned `Out`, the
/// generated `OutView<'_>` / `OwnedOutView`,
/// [`MaybeBorrowed`](::connectrpc::MaybeBorrowed), or
/// [`PreEncoded`](::connectrpc::PreEncoded) for handlers that encode a
/// non-`'static` view internally and pass the bytes across the handler
/// boundary. View bodies are not emitted for output types mapped via
/// `extern_path` (the impl would be an orphan); return owned for
/// WKT/extern outputs.
///
/// Server-streaming and bidi-streaming methods return
/// `ServiceStream<impl Encodable<Out> + Send + use<Self>>`. The
/// `use<Self>` precise-capturing clause excludes `&self`'s lifetime
/// (unary methods use `use<'a, Self>` and may borrow), so stream items
/// must be `'static`. To stream view-encoded data, encode each item
/// inside the stream body and yield
/// [`PreEncoded`](::connectrpc::PreEncoded) — see its `# Streaming
/// example` doc.
#[allow(clippy::type_complexity)]
pub trait GreeterService: Send + Sync + 'static {
    /// Handle the Expand RPC.
    fn expand(
        &self,
        ctx: ::connectrpc::RequestContext,
        request: OwnedHelloRequestView,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            ::connectrpc::ServiceStream<
                impl ::connectrpc::Encodable<
                    crate::proto::streaming::v1::HelloReply,
                > + Send + use<Self>,
            >,
        >,
    > + Send;
    /// Handle the Collect RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    fn collect<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        requests: ::connectrpc::ServiceStream<OwnedHelloRequestView>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<
                crate::proto::streaming::v1::HelloSummary,
            > + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the Chat RPC.
    fn chat(
        &self,
        ctx: ::connectrpc::RequestContext,
        requests: ::connectrpc::ServiceStream<OwnedHelloRequestView>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            ::connectrpc::ServiceStream<
                impl ::connectrpc::Encodable<
                    crate::proto::streaming::v1::HelloReply,
                > + Send + use<Self>,
            >,
        >,
    > + Send;
    /// Handle the Unary RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    fn unary<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: OwnedHelloRequestView,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<
                crate::proto::streaming::v1::HelloReply,
            > + Send + use<'a, Self>,
        >,
    > + Send;
}
/// Extension trait for registering a service implementation with a Router.
///
/// This trait is automatically implemented for all types that implement the service trait.
///
/// # Example
///
/// ```rust,ignore
/// use std::sync::Arc;
///
/// let service = Arc::new(MyServiceImpl);
/// let router = service.register(Router::new());
/// ```
pub trait GreeterServiceExt: GreeterService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(
        self: ::std::sync::Arc<Self>,
        router: ::connectrpc::Router,
    ) -> ::connectrpc::Router;
}
impl<S: GreeterService> GreeterServiceExt for S {
    fn register(
        self: ::std::sync::Arc<Self>,
        router: ::connectrpc::Router,
    ) -> ::connectrpc::Router {
        router
            .route_view_server_stream::<
                _,
                _,
                crate::proto::streaming::v1::HelloReply,
            >(
                GREETER_SERVICE_SERVICE_NAME,
                "Expand",
                ::connectrpc::view_streaming_handler_fn({
                    let svc = ::std::sync::Arc::clone(&self);
                    move |ctx, req| {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move { svc.expand(ctx, req).await }
                    }
                }),
            )
            .with_spec(GREETER_SERVICE_EXPAND_SPEC)
            .route_view_client_stream(
                GREETER_SERVICE_SERVICE_NAME,
                "Collect",
                ::connectrpc::view_client_streaming_handler_fn({
                    let svc = ::std::sync::Arc::clone(&self);
                    move |ctx, req, format| {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            svc.collect(ctx, req)
                                .await?
                                .encode::<crate::proto::streaming::v1::HelloSummary>(format)
                        }
                    }
                }),
            )
            .with_spec(GREETER_SERVICE_COLLECT_SPEC)
            .route_view_bidi_stream::<
                _,
                _,
                crate::proto::streaming::v1::HelloReply,
            >(
                GREETER_SERVICE_SERVICE_NAME,
                "Chat",
                ::connectrpc::view_bidi_streaming_handler_fn({
                    let svc = ::std::sync::Arc::clone(&self);
                    move |ctx, req| {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move { svc.chat(ctx, req).await }
                    }
                }),
            )
            .with_spec(GREETER_SERVICE_CHAT_SPEC)
            .route_view(
                GREETER_SERVICE_SERVICE_NAME,
                "Unary",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |ctx, req, format| {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            svc.unary(ctx, req)
                                .await?
                                .encode::<crate::proto::streaming::v1::HelloReply>(format)
                        }
                    })
                },
            )
            .with_spec(GREETER_SERVICE_UNARY_SPEC)
    }
}
/// Monomorphic dispatcher for `GreeterService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = GreeterServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct GreeterServiceServer<T> {
    inner: ::std::sync::Arc<T>,
}
impl<T: GreeterService> GreeterServiceServer<T> {
    /// Wrap a service implementation in a monomorphic dispatcher.
    pub fn new(service: T) -> Self {
        Self {
            inner: ::std::sync::Arc::new(service),
        }
    }
    /// Wrap an already-`Arc`'d service implementation.
    pub fn from_arc(inner: ::std::sync::Arc<T>) -> Self {
        Self { inner }
    }
}
impl<T> Clone for GreeterServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: ::std::sync::Arc::clone(&self.inner),
        }
    }
}
impl<T: GreeterService> ::connectrpc::Dispatcher for GreeterServiceServer<T> {
    #[inline]
    fn lookup(
        &self,
        path: &str,
    ) -> Option<::connectrpc::dispatcher::codegen::MethodDescriptor> {
        let method = path.strip_prefix("streaming.v1.GreeterService/")?;
        match method {
            "Expand" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::server_streaming()
                        .with_spec(GREETER_SERVICE_EXPAND_SPEC),
                )
            }
            "Collect" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::client_streaming()
                        .with_spec(GREETER_SERVICE_COLLECT_SPEC),
                )
            }
            "Chat" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::bidi_streaming()
                        .with_spec(GREETER_SERVICE_CHAT_SPEC),
                )
            }
            "Unary" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(GREETER_SERVICE_UNARY_SPEC),
                )
            }
            _ => None,
        }
    }
    fn call_unary(
        &self,
        path: &str,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::Payload,
        format: ::connectrpc::CodecFormat,
    ) -> ::connectrpc::dispatcher::codegen::UnaryResult {
        let Some(method) = path.strip_prefix("streaming.v1.GreeterService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "Unary" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = ::connectrpc::dispatcher::codegen::decode_request_view::<
                        crate::proto::streaming::v1::__buffa::view::HelloRequestView,
                    >(request.encoded()?, format)?;
                    svc.unary(ctx, req)
                        .await?
                        .encode::<crate::proto::streaming::v1::HelloReply>(format)
                })
            }
            _ => ::connectrpc::dispatcher::codegen::unimplemented_unary(path),
        }
    }
    fn call_server_streaming(
        &self,
        path: &str,
        ctx: ::connectrpc::RequestContext,
        request: ::buffa::bytes::Bytes,
        format: ::connectrpc::CodecFormat,
    ) -> ::connectrpc::dispatcher::codegen::StreamingResult {
        let Some(method) = path.strip_prefix("streaming.v1.GreeterService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "Expand" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = ::connectrpc::dispatcher::codegen::decode_request_view::<
                        crate::proto::streaming::v1::__buffa::view::HelloRequestView,
                    >(request, format)?;
                    let resp = svc.expand(ctx, req).await?;
                    Ok(
                        resp
                            .map_body(|s| ::connectrpc::dispatcher::codegen::encode_response_stream::<
                                crate::proto::streaming::v1::HelloReply,
                                _,
                                _,
                            >(s, format)),
                    )
                })
            }
            _ => ::connectrpc::dispatcher::codegen::unimplemented_streaming(path),
        }
    }
    fn call_client_streaming(
        &self,
        path: &str,
        ctx: ::connectrpc::RequestContext,
        requests: ::connectrpc::dispatcher::codegen::RequestStream,
        format: ::connectrpc::CodecFormat,
    ) -> ::connectrpc::dispatcher::codegen::UnaryResult {
        let Some(method) = path.strip_prefix("streaming.v1.GreeterService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            "Collect" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let req_stream = ::connectrpc::dispatcher::codegen::decode_view_request_stream::<
                        crate::proto::streaming::v1::__buffa::view::HelloRequestView,
                    >(requests, format);
                    svc.collect(ctx, req_stream)
                        .await?
                        .encode::<crate::proto::streaming::v1::HelloSummary>(format)
                })
            }
            _ => ::connectrpc::dispatcher::codegen::unimplemented_unary(path),
        }
    }
    fn call_bidi_streaming(
        &self,
        path: &str,
        ctx: ::connectrpc::RequestContext,
        requests: ::connectrpc::dispatcher::codegen::RequestStream,
        format: ::connectrpc::CodecFormat,
    ) -> ::connectrpc::dispatcher::codegen::StreamingResult {
        let Some(method) = path.strip_prefix("streaming.v1.GreeterService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            "Chat" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let req_stream = ::connectrpc::dispatcher::codegen::decode_view_request_stream::<
                        crate::proto::streaming::v1::__buffa::view::HelloRequestView,
                    >(requests, format);
                    let resp = svc.chat(ctx, req_stream).await?;
                    Ok(
                        resp
                            .map_body(|s| ::connectrpc::dispatcher::codegen::encode_response_stream::<
                                crate::proto::streaming::v1::HelloReply,
                                _,
                                _,
                            >(s, format)),
                    )
                })
            }
            _ => ::connectrpc::dispatcher::codegen::unimplemented_streaming(path),
        }
    }
}
/// Client for this service.
///
/// Generic over `T: ClientTransport`. For **gRPC** (HTTP/2), use
/// `Http2Connection` — it has honest `poll_ready` and composes with
/// `tower::balance` for multi-connection load balancing. For **Connect
/// over HTTP/1.1** (or unknown protocol), use `HttpClient`.
///
/// # Example (gRPC / HTTP/2)
///
/// ```rust,ignore
/// use connectrpc::client::{Http2Connection, ClientConfig};
/// use connectrpc::Protocol;
///
/// let uri: http::Uri = "http://localhost:8080".parse()?;
/// let conn = Http2Connection::connect_plaintext(uri.clone()).await?.shared(1024);
/// let config = ClientConfig::new(uri).with_protocol(Protocol::Grpc);
///
/// let client = GreeterServiceClient::new(conn, config);
/// let response = client.expand(request).await?;
/// ```
///
/// # Example (Connect / HTTP/1.1 or ALPN)
///
/// ```rust,ignore
/// use connectrpc::client::{HttpClient, ClientConfig};
///
/// let http = HttpClient::plaintext();  // cleartext http:// only
/// let config = ClientConfig::new("http://localhost:8080".parse()?);
///
/// let client = GreeterServiceClient::new(http, config);
/// let response = client.expand(request).await?;
/// ```
///
/// # Working with the response
///
/// Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
/// The `OwnedView` derefs to the view, so field access is zero-copy:
///
/// ```rust,ignore
/// let resp = client.expand(request).await?.into_view();
/// let name: &str = resp.name;  // borrow into the response buffer
/// ```
///
/// If you need the owned struct (e.g. to store or pass by value), use
/// [`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):
///
/// ```rust,ignore
/// let owned = client.expand(request).await?.into_owned();
/// ```
#[derive(Clone)]
pub struct GreeterServiceClient<T> {
    transport: T,
    config: ::connectrpc::client::ClientConfig,
}
impl<T> GreeterServiceClient<T>
where
    T: ::connectrpc::client::ClientTransport,
    <T::ResponseBody as ::http_body::Body>::Error: ::std::fmt::Display,
{
    /// Create a new client with the given transport and configuration.
    pub fn new(transport: T, config: ::connectrpc::client::ClientConfig) -> Self {
        Self { transport, config }
    }
    /// Get the client configuration.
    pub fn config(&self) -> &::connectrpc::client::ClientConfig {
        &self.config
    }
    /// Get a mutable reference to the client configuration.
    pub fn config_mut(&mut self) -> &mut ::connectrpc::client::ClientConfig {
        &mut self.config
    }
    /// Call the Expand RPC. Sends a request to /streaming.v1.GreeterService/Expand.
    pub async fn expand(
        &self,
        request: crate::proto::streaming::v1::HelloRequest,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            crate::proto::streaming::v1::__buffa::view::HelloReplyView<'static>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.expand_with_options(request, ::connectrpc::client::CallOptions::default())
            .await
    }
    /// Call the Expand RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn expand_with_options(
        &self,
        request: crate::proto::streaming::v1::HelloRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            crate::proto::streaming::v1::__buffa::view::HelloReplyView<'static>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_server_stream(
                &self.transport,
                &self.config,
                GREETER_SERVICE_SERVICE_NAME,
                "Expand",
                request,
                options,
            )
            .await
    }
    /// Call the Collect RPC. Sends a request to /streaming.v1.GreeterService/Collect.
    pub async fn collect(
        &self,
        requests: impl IntoIterator<Item = crate::proto::streaming::v1::HelloRequest>,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::streaming::v1::__buffa::view::HelloSummaryView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        self.collect_with_options(requests, ::connectrpc::client::CallOptions::default())
            .await
    }
    /// Call the Collect RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn collect_with_options(
        &self,
        requests: impl IntoIterator<Item = crate::proto::streaming::v1::HelloRequest>,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::streaming::v1::__buffa::view::HelloSummaryView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_client_stream(
                &self.transport,
                &self.config,
                GREETER_SERVICE_SERVICE_NAME,
                "Collect",
                requests,
                options,
            )
            .await
    }
    /// Call the Chat RPC. Sends a request to /streaming.v1.GreeterService/Chat.
    pub async fn chat(
        &self,
    ) -> Result<
        ::connectrpc::client::BidiStream<
            T::ResponseBody,
            crate::proto::streaming::v1::HelloRequest,
            crate::proto::streaming::v1::__buffa::view::HelloReplyView<'static>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.chat_with_options(::connectrpc::client::CallOptions::default()).await
    }
    /// Call the Chat RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn chat_with_options(
        &self,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::BidiStream<
            T::ResponseBody,
            crate::proto::streaming::v1::HelloRequest,
            crate::proto::streaming::v1::__buffa::view::HelloReplyView<'static>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_bidi_stream(
                &self.transport,
                &self.config,
                GREETER_SERVICE_SERVICE_NAME,
                "Chat",
                options,
            )
            .await
    }
    /// Call the Unary RPC. Sends a request to /streaming.v1.GreeterService/Unary.
    pub async fn unary(
        &self,
        request: crate::proto::streaming::v1::HelloRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::streaming::v1::__buffa::view::HelloReplyView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        self.unary_with_options(request, ::connectrpc::client::CallOptions::default())
            .await
    }
    /// Call the Unary RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn unary_with_options(
        &self,
        request: crate::proto::streaming::v1::HelloRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::streaming::v1::__buffa::view::HelloReplyView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                GREETER_SERVICE_SERVICE_NAME,
                "Unary",
                request,
                options,
            )
            .await
    }
}
