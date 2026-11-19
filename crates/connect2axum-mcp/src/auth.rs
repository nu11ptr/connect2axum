//! OAuth discovery for MCP clients: "discovery in, validation out".
//!
//! MCP clients that log in through OAuth find the authorization server from
//! the server's OAuth 2.0 Protected Resource Metadata (RFC 9728). This module
//! serves that document and builds the `WWW-Authenticate` challenges that
//! point to it. It never inspects a token: validating tokens, including that
//! they were issued for [`ProtectedResource::resource`], is the job of the
//! application's own authentication layer.
//!
//! Nothing here is required. An application that never calls these functions
//! serves nothing extra, and clients using a static API key header keep
//! working.

use flexstr::SharedStr;
use http::header::CONTENT_TYPE;
use http::{HeaderValue, Uri};
use serde_json::{Map, Value};
use uni_error::UniError;

use crate::error::{McpErrKind, McpResult};

const WELL_KNOWN_PREFIX: &str = "/.well-known/oauth-protected-resource";

/// The protected resource an MCP endpoint represents, and the authorization
/// servers that issue tokens for it.
#[derive(Clone, Debug)]
#[must_use]
pub struct ProtectedResource {
    resource: SharedStr,
    scheme: SharedStr,
    authority: SharedStr,
    path: SharedStr,
    authorization_servers: Vec<SharedStr>,
    scopes: Vec<SharedStr>,
    resource_name: Option<SharedStr>,
}

impl ProtectedResource {
    /// `resource_url` is the MCP endpoint's canonical URL, such as
    /// `https://api.example.com/mcp`. Clients request tokens for exactly this
    /// value, so the application's token validator must accept it as an
    /// audience. A trailing slash is removed.
    pub fn new(resource_url: impl AsRef<str>) -> McpResult<Self> {
        let resource_url = resource_url.as_ref();
        let invalid = |reason: &str| {
            UniError::from_kind_context(
                McpErrKind::InvalidConfig,
                format!("protected resource URL {resource_url:?} {reason}"),
            )
        };
        if resource_url.contains('#') {
            return Err(invalid("must not contain a fragment"));
        }
        let uri = resource_url
            .parse::<Uri>()
            .map_err(|_| invalid("is not a valid URL"))?;
        let scheme = uri
            .scheme_str()
            .filter(|scheme| matches!(*scheme, "https" | "http"))
            .ok_or_else(|| invalid("must use https or http"))?;
        let authority = uri
            .authority()
            .ok_or_else(|| invalid("must include a host"))?;
        if uri.query().is_some() {
            return Err(invalid("must not contain a query"));
        }
        let path = uri.path().trim_end_matches('/');

        Ok(Self {
            resource: format!("{scheme}://{authority}{path}").into(),
            scheme: scheme.to_owned().into(),
            authority: authority.as_str().to_owned().into(),
            path: path.to_owned().into(),
            authorization_servers: Vec::new(),
            scopes: Vec::new(),
            resource_name: None,
        })
    }

    /// Adds an authorization server issuer URL, such as your identity
    /// provider's issuer. At least one is required.
    pub fn authorization_server(mut self, issuer_url: impl Into<SharedStr>) -> Self {
        self.authorization_servers.push(issuer_url.into());
        self
    }

    /// The scopes clients should request for basic use (`scopes_supported`).
    pub fn scopes<I>(mut self, scopes: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<SharedStr>,
    {
        self.scopes = scopes.into_iter().map(Into::into).collect();
        self
    }

    /// A human-readable name for the resource.
    pub fn resource_name(mut self, name: impl Into<SharedStr>) -> Self {
        self.resource_name = Some(name.into());
        self
    }

    /// The canonical resource URL, as published in the metadata document.
    #[must_use]
    pub fn resource(&self) -> &str {
        self.resource.as_ref()
    }

    /// The path the metadata document is served at, such as
    /// `/.well-known/oauth-protected-resource/mcp`.
    #[must_use]
    pub fn metadata_path(&self) -> String {
        format!("{WELL_KNOWN_PREFIX}{}", self.path.as_ref())
    }

    /// The absolute URL of the metadata document.
    #[must_use]
    pub fn metadata_url(&self) -> String {
        format!(
            "{}://{}{}",
            self.scheme.as_ref(),
            self.authority.as_ref(),
            self.metadata_path()
        )
    }

    /// The RFC 9728 metadata document.
    #[must_use]
    pub fn metadata(&self) -> Value {
        let strings = |values: &[SharedStr]| {
            Value::Array(
                values
                    .iter()
                    .map(|value| Value::String(value.to_string()))
                    .collect(),
            )
        };
        let mut document = Map::new();
        document.insert(
            "resource".to_owned(),
            Value::String(self.resource.to_string()),
        );
        document.insert(
            "authorization_servers".to_owned(),
            strings(&self.authorization_servers),
        );
        if !self.scopes.is_empty() {
            document.insert("scopes_supported".to_owned(), strings(&self.scopes));
        }
        document.insert(
            "bearer_methods_supported".to_owned(),
            Value::Array(vec![Value::String("header".to_owned())]),
        );
        if let Some(name) = &self.resource_name {
            document.insert("resource_name".to_owned(), Value::String(name.to_string()));
        }
        Value::Object(document)
    }
}

/// Builds a router serving the resource's metadata document at
/// [`ProtectedResource::metadata_path`].
///
/// Mount it at the site root, outside any authentication layer: clients fetch
/// it before they have a token. Browser-based clients also need it covered by
/// the application's CORS policy.
pub fn protected_resource_router(resource: &ProtectedResource) -> McpResult<axum::Router> {
    if resource.authorization_servers.is_empty() {
        return Err(UniError::from_kind_context(
            McpErrKind::InvalidConfig,
            format!(
                "protected resource {} needs at least one authorization server",
                resource.resource()
            ),
        ));
    }

    let body = resource.metadata().to_string();
    Ok(axum::Router::new().route(
        &resource.metadata_path(),
        axum::routing::get(move || {
            let body = body.clone();
            async move { ([(CONTENT_TYPE, "application/json")], body) }
        }),
    ))
}

/// The `WWW-Authenticate` value for a 401 response to a request without a
/// valid token. `scope` optionally names the scopes the request needs.
pub fn bearer_challenge(
    resource: &ProtectedResource,
    scope: Option<&str>,
) -> McpResult<HeaderValue> {
    let mut parameters = vec![("resource_metadata", resource.metadata_url())];
    if let Some(scope) = scope {
        parameters.push(("scope", scope.to_owned()));
    }
    challenge(&parameters)
}

/// The `WWW-Authenticate` value for a 403 response to a token that lacks
/// `scope`, the space-separated scopes the operation needs.
pub fn insufficient_scope_challenge(
    resource: &ProtectedResource,
    scope: &str,
    description: Option<&str>,
) -> McpResult<HeaderValue> {
    let mut parameters = vec![
        ("error", "insufficient_scope".to_owned()),
        ("scope", scope.to_owned()),
        ("resource_metadata", resource.metadata_url()),
    ];
    if let Some(description) = description {
        parameters.push(("error_description", description.to_owned()));
    }
    challenge(&parameters)
}

fn challenge(parameters: &[(&str, String)]) -> McpResult<HeaderValue> {
    let parameters = parameters
        .iter()
        .map(|(name, value)| {
            let value = value.replace('\\', "\\\\").replace('"', "\\\"");
            format!("{name}=\"{value}\"")
        })
        .collect::<Vec<_>>()
        .join(", ");
    HeaderValue::from_str(&format!("Bearer {parameters}")).map_err(|_| {
        UniError::from_kind_context(
            McpErrKind::InvalidConfig,
            "WWW-Authenticate parameters must be visible ASCII",
        )
    })
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use http::{Request, StatusCode};
    use serde_json::json;
    use tower::ServiceExt as _;

    use super::{
        ProtectedResource, bearer_challenge, insufficient_scope_challenge,
        protected_resource_router,
    };

    fn resource() -> ProtectedResource {
        ProtectedResource::new("https://api.example.com/mcp/")
            .unwrap()
            .authorization_server("https://auth.example.com")
            .scopes(["mcp:tools"])
            .resource_name("Example")
    }

    #[test]
    fn derives_path_specific_metadata_location() {
        let resource = resource();

        assert_eq!(resource.resource(), "https://api.example.com/mcp");
        assert_eq!(
            resource.metadata_path(),
            "/.well-known/oauth-protected-resource/mcp"
        );
        assert_eq!(
            resource.metadata_url(),
            "https://api.example.com/.well-known/oauth-protected-resource/mcp"
        );

        let root = ProtectedResource::new("https://api.example.com:8443").unwrap();
        assert_eq!(
            root.metadata_path(),
            "/.well-known/oauth-protected-resource"
        );
        assert_eq!(
            root.metadata_url(),
            "https://api.example.com:8443/.well-known/oauth-protected-resource"
        );
    }

    #[test]
    fn rejects_unusable_resource_urls() {
        for url in [
            "api.example.com/mcp",
            "ftp://api.example.com/mcp",
            "https://api.example.com/mcp#frag",
            "https://api.example.com/mcp?x=1",
        ] {
            assert!(ProtectedResource::new(url).is_err(), "{url}");
        }
    }

    #[tokio::test]
    async fn serves_the_metadata_document() {
        let response = protected_resource_router(&resource())
            .unwrap()
            .oneshot(
                Request::builder()
                    .uri("/.well-known/oauth-protected-resource/mcp")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "application/json");
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let document: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            document,
            json!({
                "resource": "https://api.example.com/mcp",
                "authorization_servers": ["https://auth.example.com"],
                "scopes_supported": ["mcp:tools"],
                "bearer_methods_supported": ["header"],
                "resource_name": "Example"
            })
        );
    }

    #[test]
    fn router_requires_an_authorization_server() {
        let resource = ProtectedResource::new("https://api.example.com/mcp").unwrap();
        let err = protected_resource_router(&resource).unwrap_err();

        assert!(
            err.to_string()
                .contains("at least one authorization server")
        );
    }

    #[test]
    fn builds_challenges() {
        let resource = resource();

        assert_eq!(
            bearer_challenge(&resource, None).unwrap(),
            r#"Bearer resource_metadata="https://api.example.com/.well-known/oauth-protected-resource/mcp""#
        );
        assert_eq!(
            bearer_challenge(&resource, Some("mcp:tools")).unwrap(),
            r#"Bearer resource_metadata="https://api.example.com/.well-known/oauth-protected-resource/mcp", scope="mcp:tools""#
        );
        assert_eq!(
            insufficient_scope_challenge(&resource, "books:write", Some(r#"needs "write""#))
                .unwrap(),
            r#"Bearer error="insufficient_scope", scope="books:write", resource_metadata="https://api.example.com/.well-known/oauth-protected-resource/mcp", error_description="needs \"write\"""#
        );
    }
}
