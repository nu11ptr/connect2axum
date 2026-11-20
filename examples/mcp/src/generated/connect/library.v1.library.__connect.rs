///Shorthand for `OwnedView<AddBookRequestView<'static>>`.
pub type OwnedAddBookRequestView = ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::AddBookRequestView<'static>,
>;
///Shorthand for `OwnedView<BookView<'static>>`.
pub type OwnedBookView = ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::BookView<'static>,
>;
///Shorthand for `OwnedView<SearchBooksRequestView<'static>>`.
pub type OwnedSearchBooksRequestView = ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::SearchBooksRequestView<'static>,
>;
///Shorthand for `OwnedView<SearchBooksResponseView<'static>>`.
pub type OwnedSearchBooksResponseView = ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::SearchBooksResponseView<'static>,
>;
///Shorthand for `OwnedView<DeleteBookRequestView<'static>>`.
pub type OwnedDeleteBookRequestView = ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::DeleteBookRequestView<'static>,
>;
///Shorthand for `OwnedView<DeleteBookResponseView<'static>>`.
pub type OwnedDeleteBookResponseView = ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::DeleteBookResponseView<'static>,
>;
///Shorthand for `OwnedView<ClearCatalogRequestView<'static>>`.
pub type OwnedClearCatalogRequestView = ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::ClearCatalogRequestView<'static>,
>;
///Shorthand for `OwnedView<ClearCatalogResponseView<'static>>`.
pub type OwnedClearCatalogResponseView = ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::ClearCatalogResponseView<'static>,
>;
///Shorthand for `OwnedView<WatchBooksRequestView<'static>>`.
pub type OwnedWatchBooksRequestView = ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::WatchBooksRequestView<'static>,
>;
impl ::connectrpc::Encodable<crate::proto::library::v1::Book>
for crate::proto::library::v1::__buffa::view::BookView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<crate::proto::library::v1::Book>
for ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::BookView<'static>,
> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<crate::proto::library::v1::SearchBooksResponse>
for crate::proto::library::v1::__buffa::view::SearchBooksResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<crate::proto::library::v1::SearchBooksResponse>
for ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::SearchBooksResponseView<'static>,
> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<crate::proto::library::v1::DeleteBookResponse>
for crate::proto::library::v1::__buffa::view::DeleteBookResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<crate::proto::library::v1::DeleteBookResponse>
for ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::DeleteBookResponseView<'static>,
> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<crate::proto::library::v1::ClearCatalogResponse>
for crate::proto::library::v1::__buffa::view::ClearCatalogResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<crate::proto::library::v1::ClearCatalogResponse>
for ::buffa::view::OwnedView<
    crate::proto::library::v1::__buffa::view::ClearCatalogResponseView<'static>,
> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
/// Full service name for this service.
pub const LIBRARY_SERVICE_SERVICE_NAME: &str = "library.v1.LibraryService";
/// Static [`Spec`](::connectrpc::Spec) for the `AddBook` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const LIBRARY_SERVICE_ADD_BOOK_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/library.v1.LibraryService/AddBook",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `SearchBooks` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const LIBRARY_SERVICE_SEARCH_BOOKS_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/library.v1.LibraryService/SearchBooks",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `DeleteBook` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const LIBRARY_SERVICE_DELETE_BOOK_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/library.v1.LibraryService/DeleteBook",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `ClearCatalog` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const LIBRARY_SERVICE_CLEAR_CATALOG_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/library.v1.LibraryService/ClearCatalog",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `WatchBooks` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const LIBRARY_SERVICE_WATCH_BOOKS_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/library.v1.LibraryService/WatchBooks",
        ::connectrpc::StreamType::ServerStream,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Manages a small in-memory book catalog.
///
/// # Implementing handlers
///
/// Implement methods with plain `async fn`; the returned future satisfies
/// the `Send` bound automatically.
///
/// **Unary and server-streaming requests** arrive as
/// [`ServiceRequest<'_, Req>`](::connectrpc::ServiceRequest): a zero-copy
/// view of the request plus its body, valid for the duration of the call.
/// Fields are read directly (`request.name` is a `&str` into the decoded
/// buffer) and the borrow may be held across `.await` points. Anything
/// that must outlive the call — `tokio::spawn`, channels, server state,
/// or data captured by a returned response stream — takes owned data:
/// call `request.to_owned_message()` (or copy the specific fields)
/// first.
///
/// **Client-streaming and bidi requests** arrive as
/// [`InboundStream<Req>`](::connectrpc::InboundStream) — a
/// `ServiceStream` of [`StreamMessage`](::connectrpc::StreamMessage)s.
/// Each item owns its decoded buffer and is `Send + 'static`, so items
/// can be buffered or moved into spawned tasks; read fields zero-copy
/// through the generated accessor methods (`item.name()`) or `.view()`,
/// convert with `.to_owned_message()`, or yield an item back unchanged —
/// `StreamMessage<M>` implements `Encodable<M>`.
///
/// Request types resolved through `extern_path` (e.g. well-known types
/// from another crate) use the same wrappers; the crate that owns the
/// type must be generated with buffa ≥ 0.9.0 and views enabled so the
/// backing `HasMessageView` impl exists.
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
/// `use<Self>` precise-capturing clause excludes `&self`'s lifetime and
/// the request's lifetime (unary methods use `use<'a, Self>` and may
/// borrow from `&self`), so stream items must be `'static` and cannot
/// borrow from the request. To stream view-encoded data, encode each
/// item inside the stream body and yield
/// [`PreEncoded`](::connectrpc::PreEncoded) — see its `# Streaming
/// example` doc.
#[allow(clippy::type_complexity)]
pub trait LibraryService: Send + Sync + 'static {
    /// Adds a book to the catalog and returns it with its assigned ID.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn add_book<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<
            '_,
            crate::proto::library::v1::AddBookRequest,
        >,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<
                crate::proto::library::v1::Book,
            > + Send + use<'a, Self>,
        >,
    > + Send;
    /// Finds books by author or genre. This RPC has no REST binding but is still
    /// an MCP tool.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn search_books<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<
            '_,
            crate::proto::library::v1::SearchBooksRequest,
        >,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<
                crate::proto::library::v1::SearchBooksResponse,
            > + Send + use<'a, Self>,
        >,
    > + Send;
    /// Removes a book. Over MCP, the user confirms each call (see
    /// connect2mcp.yaml).
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn delete_book<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<
            '_,
            crate::proto::library::v1::DeleteBookRequest,
        >,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<
                crate::proto::library::v1::DeleteBookResponse,
            > + Send + use<'a, Self>,
        >,
    > + Send;
    /// Removes every book. Excluded from MCP in connect2mcp.yaml.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn clear_catalog<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<
            '_,
            crate::proto::library::v1::ClearCatalogRequest,
        >,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<
                crate::proto::library::v1::ClearCatalogResponse,
            > + Send + use<'a, Self>,
        >,
    > + Send;
    /// Streams every book. Streaming RPCs are not MCP tools.
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call (until the response stream is returned);
    /// message fields are read directly on it (zero-copy). Data the
    /// returned stream needs must be copied out or converted via
    /// `.to_owned_message()`.
    fn watch_books(
        &self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<
            '_,
            crate::proto::library::v1::WatchBooksRequest,
        >,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            ::connectrpc::ServiceStream<
                impl ::connectrpc::Encodable<
                    crate::proto::library::v1::Book,
                > + Send + use<Self>,
            >,
        >,
    > + Send;
}
/// Extension trait for registering a service implementation with a Router.
///
/// This trait is automatically implemented for all types that implement the service trait.
/// Prefer [`Router::add_service`](::connectrpc::Router::add_service) for
/// top-down registration; `register` remains available for compatibility
/// and cases where the service-first call shape is more convenient.
///
/// # Example
///
/// ```rust,ignore
/// use std::sync::Arc;
///
/// let service = Arc::new(MyServiceImpl);
/// let router = service.register(Router::new());
/// ```
pub trait LibraryServiceExt: LibraryService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(
        self: ::std::sync::Arc<Self>,
        router: ::connectrpc::Router,
    ) -> ::connectrpc::Router;
}
impl<S: LibraryService> LibraryServiceExt for S {
    fn register(
        self: ::std::sync::Arc<Self>,
        router: ::connectrpc::Router,
    ) -> ::connectrpc::Router {
        router
            .route_view(
                LIBRARY_SERVICE_SERVICE_NAME,
                "AddBook",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            crate::proto::library::v1::__buffa::view::AddBookRequestView<
                                'static,
                            >,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                crate::proto::library::v1::AddBookRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.add_book(ctx, sreq)
                                .await?
                                .encode::<crate::proto::library::v1::Book>(format)
                        }
                    })
                },
            )
            .with_spec(LIBRARY_SERVICE_ADD_BOOK_SPEC)
            .route_view(
                LIBRARY_SERVICE_SERVICE_NAME,
                "SearchBooks",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            crate::proto::library::v1::__buffa::view::SearchBooksRequestView<
                                'static,
                            >,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                crate::proto::library::v1::SearchBooksRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.search_books(ctx, sreq)
                                .await?
                                .encode::<
                                    crate::proto::library::v1::SearchBooksResponse,
                                >(format)
                        }
                    })
                },
            )
            .with_spec(LIBRARY_SERVICE_SEARCH_BOOKS_SPEC)
            .route_view(
                LIBRARY_SERVICE_SERVICE_NAME,
                "DeleteBook",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            crate::proto::library::v1::__buffa::view::DeleteBookRequestView<
                                'static,
                            >,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                crate::proto::library::v1::DeleteBookRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.delete_book(ctx, sreq)
                                .await?
                                .encode::<
                                    crate::proto::library::v1::DeleteBookResponse,
                                >(format)
                        }
                    })
                },
            )
            .with_spec(LIBRARY_SERVICE_DELETE_BOOK_SPEC)
            .route_view(
                LIBRARY_SERVICE_SERVICE_NAME,
                "ClearCatalog",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            crate::proto::library::v1::__buffa::view::ClearCatalogRequestView<
                                'static,
                            >,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                crate::proto::library::v1::ClearCatalogRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.clear_catalog(ctx, sreq)
                                .await?
                                .encode::<
                                    crate::proto::library::v1::ClearCatalogResponse,
                                >(format)
                        }
                    })
                },
            )
            .with_spec(LIBRARY_SERVICE_CLEAR_CATALOG_SPEC)
            .route_view_server_stream::<
                _,
                _,
                crate::proto::library::v1::Book,
            >(
                LIBRARY_SERVICE_SERVICE_NAME,
                "WatchBooks",
                ::connectrpc::view_streaming_handler_fn({
                    let svc = ::std::sync::Arc::clone(&self);
                    move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            crate::proto::library::v1::__buffa::view::WatchBooksRequestView<
                                'static,
                            >,
                        >|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                crate::proto::library::v1::WatchBooksRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.watch_books(ctx, sreq).await
                        }
                    }
                }),
            )
            .with_spec(LIBRARY_SERVICE_WATCH_BOOKS_SPEC)
    }
}
/// Type-inference marker used by [`Router::add_service`](::connectrpc::Router::add_service).
#[doc(hidden)]
pub struct LibraryServiceRegisterMarker;
impl<S: LibraryService> ::connectrpc::ServiceRegister<LibraryServiceRegisterMarker>
for ::std::sync::Arc<S> {
    fn register_service(self, router: ::connectrpc::Router) -> ::connectrpc::Router {
        <S as LibraryServiceExt>::register(self, router)
    }
}
/// Monomorphic dispatcher for `LibraryService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = LibraryServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct LibraryServiceServer<T> {
    inner: ::std::sync::Arc<T>,
}
impl<T: LibraryService> LibraryServiceServer<T> {
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
impl<T> Clone for LibraryServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: ::std::sync::Arc::clone(&self.inner),
        }
    }
}
impl<T: LibraryService> ::connectrpc::Dispatcher for LibraryServiceServer<T> {
    #[inline]
    fn lookup(
        &self,
        path: &str,
    ) -> Option<::connectrpc::dispatcher::codegen::MethodDescriptor> {
        let method = path.strip_prefix("library.v1.LibraryService/")?;
        match method {
            "AddBook" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(LIBRARY_SERVICE_ADD_BOOK_SPEC),
                )
            }
            "SearchBooks" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(LIBRARY_SERVICE_SEARCH_BOOKS_SPEC),
                )
            }
            "DeleteBook" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(LIBRARY_SERVICE_DELETE_BOOK_SPEC),
                )
            }
            "ClearCatalog" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(LIBRARY_SERVICE_CLEAR_CATALOG_SPEC),
                )
            }
            "WatchBooks" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::server_streaming()
                        .with_spec(LIBRARY_SERVICE_WATCH_BOOKS_SPEC),
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
        let Some(method) = path.strip_prefix("library.v1.LibraryService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "AddBook" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        crate::proto::library::v1::AddBookRequest,
                    >(request.encoded()?, format)?;
                    let req: crate::proto::library::v1::__buffa::view::AddBookRequestView<
                        '_,
                    > = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        crate::proto::library::v1::AddBookRequest,
                    >::from_parts(&req, &body);
                    svc.add_book(ctx, req)
                        .await?
                        .encode::<crate::proto::library::v1::Book>(format)
                })
            }
            "SearchBooks" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        crate::proto::library::v1::SearchBooksRequest,
                    >(request.encoded()?, format)?;
                    let req: crate::proto::library::v1::__buffa::view::SearchBooksRequestView<
                        '_,
                    > = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        crate::proto::library::v1::SearchBooksRequest,
                    >::from_parts(&req, &body);
                    svc.search_books(ctx, req)
                        .await?
                        .encode::<crate::proto::library::v1::SearchBooksResponse>(format)
                })
            }
            "DeleteBook" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        crate::proto::library::v1::DeleteBookRequest,
                    >(request.encoded()?, format)?;
                    let req: crate::proto::library::v1::__buffa::view::DeleteBookRequestView<
                        '_,
                    > = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        crate::proto::library::v1::DeleteBookRequest,
                    >::from_parts(&req, &body);
                    svc.delete_book(ctx, req)
                        .await?
                        .encode::<crate::proto::library::v1::DeleteBookResponse>(format)
                })
            }
            "ClearCatalog" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        crate::proto::library::v1::ClearCatalogRequest,
                    >(request.encoded()?, format)?;
                    let req: crate::proto::library::v1::__buffa::view::ClearCatalogRequestView<
                        '_,
                    > = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        crate::proto::library::v1::ClearCatalogRequest,
                    >::from_parts(&req, &body);
                    svc.clear_catalog(ctx, req)
                        .await?
                        .encode::<
                            crate::proto::library::v1::ClearCatalogResponse,
                        >(format)
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
        let Some(method) = path.strip_prefix("library.v1.LibraryService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "WatchBooks" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        crate::proto::library::v1::WatchBooksRequest,
                    >(request, format)?;
                    let req: crate::proto::library::v1::__buffa::view::WatchBooksRequestView<
                        '_,
                    > = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        crate::proto::library::v1::WatchBooksRequest,
                    >::from_parts(&req, &body);
                    let resp = svc.watch_books(ctx, req).await?;
                    Ok(
                        resp
                            .map_body(|s| ::connectrpc::dispatcher::codegen::encode_response_stream::<
                                crate::proto::library::v1::Book,
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
        let Some(method) = path.strip_prefix("library.v1.LibraryService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
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
        let Some(method) = path.strip_prefix("library.v1.LibraryService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
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
/// let client = LibraryServiceClient::new(conn, config);
/// let response = client.add_book(request).await?;
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
/// let client = LibraryServiceClient::new(http, config);
/// let response = client.add_book(request).await?;
/// ```
///
/// # Working with the response
///
/// Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
/// [`view()`](::connectrpc::client::UnaryResponse::view) borrows the response
/// message, so field access is zero-copy:
///
/// ```rust,ignore
/// let resp = client.add_book(request).await?;
/// let name: &str = resp.view().name;  // borrow into the response buffer
/// ```
///
/// If you need the owned struct (e.g. to store or pass by value), use
/// [`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):
///
/// ```rust,ignore
/// let owned = client.add_book(request).await?.into_owned();
/// ```
///
/// [`into_view()`](::connectrpc::client::UnaryResponse::into_view) keeps the
/// zero-copy decoded body (an `OwnedView`) without copying; field access on it
/// goes through `.reborrow()`. Streaming responses yield one
/// [`StreamMessage`](::connectrpc::StreamMessage) per received message from
/// `.message().await` — read fields zero-copy through the generated accessor
/// methods (`msg.name()`) or `.view()`, or convert with `.to_owned_message()`.
#[derive(Clone)]
pub struct LibraryServiceClient<T> {
    transport: T,
    config: ::connectrpc::client::ClientConfig,
}
impl<T> LibraryServiceClient<T>
where
    T: ::connectrpc::client::ClientTransport,
    <T::ResponseBody as ::connectrpc::http_body::Body>::Error: ::std::fmt::Display,
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
    /// Call the AddBook RPC. Sends a request to /library.v1.LibraryService/AddBook.
    pub async fn add_book(
        &self,
        request: crate::proto::library::v1::AddBookRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::library::v1::__buffa::view::BookView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        self.add_book_with_options(request, ::connectrpc::client::CallOptions::default())
            .await
    }
    /// Call the AddBook RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn add_book_with_options(
        &self,
        request: crate::proto::library::v1::AddBookRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::library::v1::__buffa::view::BookView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                LIBRARY_SERVICE_ADD_BOOK_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the SearchBooks RPC. Sends a request to /library.v1.LibraryService/SearchBooks.
    pub async fn search_books(
        &self,
        request: crate::proto::library::v1::SearchBooksRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::library::v1::__buffa::view::SearchBooksResponseView<
                    'static,
                >,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        self.search_books_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the SearchBooks RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn search_books_with_options(
        &self,
        request: crate::proto::library::v1::SearchBooksRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::library::v1::__buffa::view::SearchBooksResponseView<
                    'static,
                >,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                LIBRARY_SERVICE_SEARCH_BOOKS_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the DeleteBook RPC. Sends a request to /library.v1.LibraryService/DeleteBook.
    pub async fn delete_book(
        &self,
        request: crate::proto::library::v1::DeleteBookRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::library::v1::__buffa::view::DeleteBookResponseView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        self.delete_book_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the DeleteBook RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn delete_book_with_options(
        &self,
        request: crate::proto::library::v1::DeleteBookRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::library::v1::__buffa::view::DeleteBookResponseView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                LIBRARY_SERVICE_DELETE_BOOK_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the ClearCatalog RPC. Sends a request to /library.v1.LibraryService/ClearCatalog.
    pub async fn clear_catalog(
        &self,
        request: crate::proto::library::v1::ClearCatalogRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::library::v1::__buffa::view::ClearCatalogResponseView<
                    'static,
                >,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        self.clear_catalog_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the ClearCatalog RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn clear_catalog_with_options(
        &self,
        request: crate::proto::library::v1::ClearCatalogRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                crate::proto::library::v1::__buffa::view::ClearCatalogResponseView<
                    'static,
                >,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                LIBRARY_SERVICE_CLEAR_CATALOG_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the WatchBooks RPC. Sends a request to /library.v1.LibraryService/WatchBooks.
    pub async fn watch_books(
        &self,
        request: crate::proto::library::v1::WatchBooksRequest,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            crate::proto::library::v1::__buffa::view::BookView<'static>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.watch_books_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the WatchBooks RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn watch_books_with_options(
        &self,
        request: crate::proto::library::v1::WatchBooksRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            crate::proto::library::v1::__buffa::view::BookView<'static>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_server_stream(
                &self.transport,
                &self.config,
                LIBRARY_SERVICE_WATCH_BOOKS_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
}
