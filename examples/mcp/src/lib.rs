#![allow(refining_impl_trait)]

use std::sync::{Arc, Mutex};

use axum::Router;
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
    AddBookRequest, Book, DeleteBookRequest, DeleteBookResponse, SearchBooksRequest,
    SearchBooksResponse, WatchBooksRequest,
};

/// An in-memory book catalog.
#[derive(Debug, Default)]
pub struct Library {
    books: Mutex<Vec<Book>>,
}

impl connect::library::v1::LibraryService for Library {
    async fn add_book<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, AddBookRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<Book> + Send + use<'a>> {
        let request = request.to_owned_message();
        if request.title.is_empty() || request.author.is_empty() {
            return Err(ConnectError::invalid_argument(
                "title and author are required",
            ));
        }

        let mut books = self.books.lock().expect("library lock");
        let book = Book {
            id: format!("book-{}", books.len() + 1),
            title: request.title,
            author: request.author,
            genre: request.genre,
            tags: request.tags,
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
    let rest = rest::library_service_rest::make_router(library.clone());
    let mcp = connect2axum_mcp::make_router(
        connect2axum_mcp::McpConfig::new("library", env!("CARGO_PKG_VERSION"))
            .title("Library catalog"),
        [mcp::library_service_mcp::make_tools(library.clone())],
    )
    .expect("MCP tool names are unique");
    let connect = library.register(connectrpc::Router::new());

    Router::new()
        .nest("/v1", rest)
        .merge(mcp)
        .fallback_service(connect.into_axum_service())
}

#[cfg(test)]
mod tests;
