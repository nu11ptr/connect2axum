#![allow(refining_impl_trait)]

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::response::IntoResponse as _;
use connectrpc::{
    ConnectError, RequestContext, Response, ServiceRequest, ServiceResult, ServiceStream,
};

#[rustfmt::skip]
#[path = "generated/connect/mod.rs"]
pub mod connect;

#[rustfmt::skip]
#[path = "generated/buffa/mod.rs"]
pub mod proto;

#[rustfmt::skip]
#[path = "generated/connect2axum/library/v1/library.connect2rest.rs"]
pub mod rest;

#[rustfmt::skip]
#[path = "generated/connect2axum/library/v1/library.connect2mcp.rs"]
pub mod mcp;

use connect::library::v1::LibraryServiceExt as _;
use proto::library::v1::__buffa::oneof::search_books_request::Filter;
use proto::library::v1::{
    AddBookRequest, Book, ClearCatalogRequest, ClearCatalogResponse, DeleteBookRequest,
    DeleteBookResponse, SearchBooksRequest, SearchBooksResponse, WatchBooksRequest,
};

/// An in-memory book catalog.
#[derive(Debug, Default)]
pub struct Library {
    books: Mutex<Vec<Book>>,
}

impl connect::library::v1::LibraryService for Library {
    async fn add_book<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, AddBookRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<Book> + Send + use<'a>> {
        let request = request.to_owned_message();
        if request.title.is_empty() || request.author.is_empty() {
            return Err(ConnectError::invalid_argument(
                "title and author are required",
            ));
        }

        let mut tags = request.tags;
        // An authentication layer in front of the route put the caller here.
        if let Some(User(user)) = ctx.extensions().get::<User>() {
            tags.insert("added_by".to_owned(), user.clone());
        }

        let mut books = self.books.lock().expect("library lock");
        let book = Book {
            id: format!("book-{}", books.len() + 1),
            title: request.title,
            author: request.author,
            genre: request.genre,
            tags,
            publisher: request.publisher,
            ..Default::default()
        };
        books.push(book.clone());
        Response::ok(book)
    }

    async fn search_books<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, SearchBooksRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<SearchBooksResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let books = self.books.lock().expect("library lock");
        let limit = match request.limit {
            0 => usize::MAX,
            limit => limit as usize,
        };
        let books = books
            .iter()
            .filter(|book| match &request.filter {
                None => true,
                Some(Filter::Author(author)) => &book.author == author,
                Some(Filter::Genre(genre)) => book.genre == *genre,
            })
            .take(limit)
            .cloned()
            .collect();

        Response::ok(SearchBooksResponse {
            books,
            ..Default::default()
        })
    }

    async fn delete_book<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, DeleteBookRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<DeleteBookResponse> + Send + use<'a>> {
        let mut books = self.books.lock().expect("library lock");
        let before = books.len();
        books.retain(|book| book.id != request.id);
        if books.len() == before {
            return Err(ConnectError::not_found(format!(
                "no book with id {}",
                request.id
            )));
        }
        Response::ok(DeleteBookResponse::default())
    }

    async fn clear_catalog<'a>(
        &'a self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, ClearCatalogRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<ClearCatalogResponse> + Send + use<'a>> {
        self.books.lock().expect("library lock").clear();
        Response::ok(ClearCatalogResponse::default())
    }

    async fn watch_books(
        &self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, WatchBooksRequest>,
    ) -> ServiceResult<ServiceStream<Book>> {
        let books = self.books.lock().expect("library lock").clone();
        Response::stream_ok(futures_util::stream::iter(books.into_iter().map(Ok)))
    }
}

/// The example application: Connect, REST under `/v1`, and MCP at `/mcp`.
pub fn app() -> Router {
    app_with(Arc::new(Library::default()))
}

pub fn app_with(library: Arc<Library>) -> Router {
    let mcp = mcp_router(library.clone());
    routes(library, mcp)
}

/// The authenticated caller, placed in request extensions by the auth layer.
#[derive(Clone, Debug)]
pub struct User(pub String);

/// The example application with the MCP route behind a bearer-token check,
/// plus the OAuth discovery document MCP clients use to find the
/// authorization server.
///
/// Validating the token is the application's job; here a fixed token stands
/// in for a real validator such as a JWT check against the identity
/// provider.
pub fn secured_app_with(
    library: Arc<Library>,
    resource: &connect2axum_mcp::ProtectedResource,
    valid_token: &'static str,
) -> Router {
    let challenge = connect2axum_mcp::bearer_challenge(resource, None).expect("valid challenge");
    let mcp = mcp_router(library.clone()).layer(axum::middleware::from_fn(
        move |mut request: axum::extract::Request, next: axum::middleware::Next| {
            let challenge = challenge.clone();
            async move {
                let token = request
                    .headers()
                    .get(http::header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.strip_prefix("Bearer "));
                if token != Some(valid_token) {
                    return (
                        http::StatusCode::UNAUTHORIZED,
                        [(http::header::WWW_AUTHENTICATE, challenge)],
                    )
                        .into_response();
                }
                request.extensions_mut().insert(User("reader".to_owned()));
                next.run(request).await
            }
        },
    ));
    let discovery = connect2axum_mcp::protected_resource_router(resource)
        .expect("resource names an authorization server");

    routes(library, mcp).merge(discovery)
}

fn mcp_router(library: Arc<Library>) -> Router {
    connect2axum_mcp::make_router(
        connect2axum_mcp::McpConfig::new("library", env!("CARGO_PKG_VERSION"))
            .title("Library catalog"),
        [mcp::library_service_mcp::make_tools(library)],
    )
    .expect("MCP tool names are unique")
}

fn routes(library: Arc<Library>, mcp: Router) -> Router {
    let rest = rest::library_service_rest::make_router(library.clone());
    let connect = library.register(connectrpc::Router::new());

    Router::new()
        .nest("/v1", rest)
        .merge(mcp)
        .fallback_service(connect.into_axum_service())
}

#[cfg(test)]
mod tests;
