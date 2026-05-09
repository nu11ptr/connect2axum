//! Small axum router for serving an embedded Scalar API reference.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use bytes::Bytes;
use http::header::{CACHE_CONTROL, CONTENT_TYPE};

#[cfg(docsrs)]
const SCALAR_JS: &str = "";
#[cfg(not(docsrs))]
const SCALAR_JS: &str = include_str!(concat!(env!("OUT_DIR"), "/scalar-api-reference.js"));

/// The Scalar API Reference version embedded by this crate.
pub const SCALAR_VERSION: &str = env!("CONNECT2AXUM_SCALAR_VERSION");

/// Route and page options for the embedded Scalar API reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScalarOptions {
    /// Route path for the Scalar HTML page.
    pub docs_path: String,
    /// Route path where the OpenAPI JSON document is served.
    pub spec_path: String,
    /// Route path where the embedded Scalar JavaScript bundle is served.
    pub js_path: String,
    /// Browser page title for the Scalar HTML page.
    pub title: String,
}

impl Default for ScalarOptions {
    fn default() -> Self {
        Self {
            docs_path: "/scalar".to_owned(),
            spec_path: "/openapi.json".to_owned(),
            js_path: "/scalar/scalar.js".to_owned(),
            title: "API Reference".to_owned(),
        }
    }
}

/// Builds a router with `/scalar`, `/openapi.json`, and the embedded Scalar JS.
pub fn router(spec_json: &'static str) -> Router {
    router_with_options(
        Bytes::from_static(spec_json.as_bytes()),
        ScalarOptions::default(),
    )
}

/// Builds a router with custom paths and page title.
pub fn router_with_options(spec_json: impl Into<Bytes>, options: ScalarOptions) -> Router {
    let ScalarOptions {
        docs_path,
        spec_path,
        js_path,
        title,
    } = options;
    let state = ScalarState {
        spec_json: spec_json.into(),
        spec_path: Arc::from(spec_path),
        js_path: Arc::from(js_path),
        title: Arc::from(title),
    };

    Router::new()
        .route(&docs_path, get(scalar_docs))
        .route(state.spec_path.as_ref(), get(openapi_json))
        .route(state.js_path.as_ref(), get(scalar_js))
        .with_state(state)
}

#[derive(Clone)]
struct ScalarState {
    spec_json: Bytes,
    spec_path: Arc<str>,
    js_path: Arc<str>,
    title: Arc<str>,
}

async fn scalar_docs(State(state): State<ScalarState>) -> Html<String> {
    Html(html_page(&state))
}

async fn openapi_json(State(state): State<ScalarState>) -> impl IntoResponse {
    (
        [
            (CONTENT_TYPE, "application/json; charset=utf-8"),
            (CACHE_CONTROL, "no-store"),
        ],
        Body::from(state.spec_json.clone()),
    )
}

async fn scalar_js() -> impl IntoResponse {
    (
        [
            (CONTENT_TYPE, "application/javascript; charset=utf-8"),
            (CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        SCALAR_JS,
    )
}

fn html_page(state: &ScalarState) -> String {
    let title = escape_html(&state.title);
    let spec_path = escape_html_attr(&state.spec_path);
    let js_path = escape_html_attr(&state.js_path);

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  <style>
    body {{
      margin: 0;
    }}
  </style>
</head>
<body>
  <script id="api-reference" data-url="{spec_path}"></script>
  <script src="{js_path}"></script>
</body>
</html>
"#
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attr(value: &str) -> String {
    escape_html(value).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::{ScalarState, escape_html_attr, html_page};
    use bytes::Bytes;
    use std::sync::Arc;

    #[test]
    fn escapes_html_attributes() {
        assert_eq!(
            escape_html_attr(r#"/docs?x="<&>"#),
            "/docs?x=&quot;&lt;&amp;&gt;"
        );
    }

    #[test]
    fn page_points_scalar_at_spec_and_embedded_script() {
        let page = html_page(&ScalarState {
            spec_json: Bytes::new(),
            spec_path: Arc::from("/openapi.json"),
            js_path: Arc::from("/scalar/scalar.js"),
            title: Arc::from("Docs"),
        });

        assert!(page.contains(r#"data-url="/openapi.json""#));
        assert!(page.contains(r#"src="/scalar/scalar.js""#));
    }
}
