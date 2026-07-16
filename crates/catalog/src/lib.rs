//! # catalog
//!
//! The built-in connector catalog. Given a connector *name* and the server's
//! options, it produces a ready-to-run [`SourceSpec`]. This is what lets
//! standard connectors mount out-of-the-box — the caller supplies only
//! credentials/config, and the bundled spec does the rest:
//!
//! ```text
//! CREATE SERVER stripe OPTIONS (connector 'stripe', api_key 'sk_...');
//! CREATE SERVER gh     OPTIONS (connector 'github', owner 'acme', repo 'app');
//! CREATE SERVER myapi  OPTIONS (connector 'openapi', spec '...', token '...');
//! ```
//!
//! Adding a new standard connector = bundle its spec and add one match arm.

use std::collections::HashMap;

use github_connector::{github_spec, GithubConfig};
use rest_connector::{AuthSpec, ImportOptions, SourceSpec};
use stripe_connector::stripe_spec;

/// Something went wrong resolving a named connector.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("unknown connector '{name}' (known: {known})")]
    Unknown { name: String, known: String },
    #[error("connector '{connector}' requires the '{option}' option")]
    MissingOption { connector: String, option: String },
    #[error("could not build spec for '{connector}': {message}")]
    InvalidSpec { connector: String, message: String },
}

/// The built-in connector names.
pub fn list() -> &'static [&'static str] {
    &["stripe", "github", "openapi"]
}

/// Build a [`SourceSpec`] for the named connector from `options`.
pub fn spec_for(name: &str, options: &HashMap<String, String>) -> Result<SourceSpec, CatalogError> {
    let get = |k: &str| options.get(k).map(String::as_str);
    let require = |k: &str| {
        get(k).ok_or_else(|| CatalogError::MissingOption {
            connector: name.to_string(),
            option: k.to_string(),
        })
    };

    match name {
        "stripe" => Ok(stripe_spec(require("api_key")?, get("base_url"))),

        "github" => Ok(github_spec(&GithubConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.github.com")
                .to_string(),
            owner: require("owner")?.to_string(),
            repo: get("repo").unwrap_or_default().to_string(),
            token: get("token").map(str::to_string),
        })),

        // Bring-your-own API: generate a spec from an OpenAPI document.
        "openapi" => {
            let doc = require("spec")?;
            let auth = match get("token").or_else(|| get("api_key")) {
                Some(t) => AuthSpec::Bearer {
                    token: t.to_string(),
                },
                None => AuthSpec::None,
            };
            let opts = ImportOptions {
                auth,
                base_url: get("base_url").map(str::to_string),
                ..Default::default()
            };
            SourceSpec::from_openapi_json(doc, opts).map_err(|e| CatalogError::InvalidSpec {
                connector: name.to_string(),
                message: e.to_string(),
            })
        }

        other => Err(CatalogError::Unknown {
            name: other.to_string(),
            known: list().join(", "),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn stripe_needs_only_an_api_key() {
        let spec = spec_for("stripe", &opts(&[("api_key", "sk_test_x")])).unwrap();
        assert_eq!(spec.name, "stripe");
        assert!(spec.table("charges").is_some());
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
    }

    #[test]
    fn github_needs_an_owner() {
        let spec = spec_for(
            "github",
            &opts(&[("owner", "octocat"), ("repo", "Hello-World")]),
        )
        .unwrap();
        assert_eq!(spec.name, "github");
        assert!(spec.table("repos").is_some());
    }

    #[test]
    fn openapi_imports_a_document_with_optional_auth() {
        let doc = serde_json::json!({
            "servers": [{"url": "https://x"}],
            "paths": {"/t": {"get": {"responses": {"200": {"content":
                {"application/json": {"schema": {"type": "array", "items":
                    {"type": "object", "properties": {"id": {"type": "string"}}}}}}}}}}}
        })
        .to_string();
        let spec = spec_for("openapi", &opts(&[("spec", doc.as_str()), ("token", "t")])).unwrap();
        assert!(spec.table("t").is_some());
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        // Without a token, auth defaults to none.
        let spec2 = spec_for("openapi", &opts(&[("spec", doc.as_str())])).unwrap();
        assert!(matches!(spec2.auth, AuthSpec::None));
    }

    #[test]
    fn missing_required_option_errors() {
        let err = spec_for("stripe", &opts(&[])).unwrap_err();
        assert!(matches!(err, CatalogError::MissingOption { .. }));
        assert!(err.to_string().contains("api_key"));
    }

    #[test]
    fn invalid_openapi_errors() {
        let err = spec_for("openapi", &opts(&[("spec", "not json")])).unwrap_err();
        assert!(matches!(err, CatalogError::InvalidSpec { .. }));
    }

    #[test]
    fn unknown_connector_errors_and_lists_known() {
        let err = spec_for("salesforce", &opts(&[])).unwrap_err();
        assert!(matches!(err, CatalogError::Unknown { .. }));
        assert!(err.to_string().contains("stripe"));
        assert!(!list().is_empty());
    }
}
