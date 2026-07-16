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

use asana_connector::{asana_spec, AsanaConfig};
use contentful_connector::{contentful_spec, ContentfulConfig};
use freshdesk_connector::{freshdesk_spec, FreshdeskConfig};
use github_connector::{github_spec, GithubConfig};
use gitlab_connector::{gitlab_spec, GitLabConfig};
use intercom_connector::{intercom_spec, IntercomConfig};
use pagerduty_connector::{pagerduty_spec, PagerDutyConfig};
use pipedrive_connector::{pipedrive_spec, PipedriveConfig};
use rest_connector::{AuthSpec, ImportOptions, SourceSpec};
use sentry_connector::{sentry_spec, SentryConfig};
use shopify_connector::{shopify_spec, ShopifyConfig};
use stripe_connector::stripe_spec;
use zendesk_connector::{zendesk_spec, ZendeskConfig};

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
    &[
        "stripe",
        "github",
        "gitlab",
        "zendesk",
        "pagerduty",
        "freshdesk",
        "contentful",
        "asana",
        "shopify",
        "intercom",
        "pipedrive",
        "sentry",
        "openapi",
    ]
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

        "gitlab" => Ok(gitlab_spec(&GitLabConfig {
            base_url: get("base_url").unwrap_or("https://gitlab.com").to_string(),
            token: get("token").map(str::to_string),
        })),

        "zendesk" => Ok(zendesk_spec(&ZendeskConfig {
            base_url: require("base_url")?.to_string(),
            email: require("email")?.to_string(),
            api_token: require("api_token")?.to_string(),
        })),

        "pagerduty" => Ok(pagerduty_spec(&PagerDutyConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.pagerduty.com")
                .to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "freshdesk" => Ok(freshdesk_spec(&FreshdeskConfig {
            base_url: require("base_url")?.to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "contentful" => Ok(contentful_spec(&ContentfulConfig {
            base_url: require("base_url")?.to_string(),
            access_token: require("access_token")?.to_string(),
        })),

        "asana" => Ok(asana_spec(&AsanaConfig {
            base_url: get("base_url")
                .unwrap_or("https://app.asana.com/api/1.0")
                .to_string(),
            token: get("token").map(str::to_string),
        })),

        "shopify" => Ok(shopify_spec(&ShopifyConfig {
            base_url: require("base_url")?.to_string(),
            access_token: require("access_token")?.to_string(),
        })),

        "intercom" => Ok(intercom_spec(&IntercomConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.intercom.io")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "pipedrive" => Ok(pipedrive_spec(&PipedriveConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.pipedrive.com/v1")
                .to_string(),
            api_token: require("api_token")?.to_string(),
        })),

        "sentry" => Ok(sentry_spec(&SentryConfig {
            base_url: get("base_url")
                .unwrap_or("https://sentry.io/api/0")
                .to_string(),
            token: require("token")?.to_string(),
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
    fn built_in_connectors_resolve_from_options() {
        // GitLab and PagerDuty default their base_url; the rest require it.
        let gl = spec_for("gitlab", &opts(&[("token", "t")])).unwrap();
        assert_eq!(gl.name, "gitlab");
        assert!(gl.table("projects").is_some());

        let zd = spec_for(
            "zendesk",
            &opts(&[
                ("base_url", "https://acme.zendesk.com"),
                ("email", "a@b.c"),
                ("api_token", "t"),
            ]),
        )
        .unwrap();
        assert!(zd.table("tickets").is_some());

        let pd = spec_for("pagerduty", &opts(&[("api_key", "k")])).unwrap();
        assert!(pd.table("incidents").is_some());

        let fd = spec_for(
            "freshdesk",
            &opts(&[("base_url", "https://acme.freshdesk.com"), ("api_key", "k")]),
        )
        .unwrap();
        assert!(fd.table("tickets").is_some());

        let cf = spec_for(
            "contentful",
            &opts(&[
                (
                    "base_url",
                    "https://cdn.contentful.com/spaces/s/environments/master",
                ),
                ("access_token", "t"),
            ]),
        )
        .unwrap();
        assert!(cf.table("entries").is_some());

        // A required option is enforced for these too.
        assert!(matches!(
            spec_for("zendesk", &opts(&[("email", "a@b.c")])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
    }

    #[test]
    fn batch1_connectors_resolve_from_options() {
        // Asana defaults its base_url and token is optional.
        let asana = spec_for("asana", &opts(&[])).unwrap();
        assert_eq!(asana.name, "asana");
        assert!(asana.table("projects").is_some());

        let shopify = spec_for(
            "shopify",
            &opts(&[
                ("base_url", "https://acme.myshopify.com/admin/api/2024-01"),
                ("access_token", "t"),
            ]),
        )
        .unwrap();
        assert!(shopify.table("products").is_some());

        let intercom = spec_for("intercom", &opts(&[("token", "t")])).unwrap();
        assert!(intercom.table("contacts").is_some());

        let pipedrive = spec_for("pipedrive", &opts(&[("api_token", "t")])).unwrap();
        assert!(pipedrive.table("deals").is_some());

        let sentry = spec_for("sentry", &opts(&[("token", "t")])).unwrap();
        assert!(sentry.table("projects").is_some());

        // Required options are enforced.
        assert!(matches!(
            spec_for("shopify", &opts(&[("access_token", "t")])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
        assert!(matches!(
            spec_for("intercom", &opts(&[])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
        assert!(matches!(
            spec_for("pipedrive", &opts(&[])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
        assert!(matches!(
            spec_for("sentry", &opts(&[])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
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
