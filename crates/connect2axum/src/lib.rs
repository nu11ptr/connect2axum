//! Runtime helpers for generated REST/OpenAPI endpoint wrappers over
//! ConnectRPC services.
//!
//! Phase 1 intentionally exposes only a tiny marker API while the workspace and
//! codegen plugin shell are established.

/// The crate version, as declared by Cargo.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn exposes_package_version() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}
