//! # stripe-connector
//!
//! Stripe as an out-of-the-box connector. The `SourceSpec` — 11 core Stripe
//! collections (customers, charges, invoices, subscriptions, …) with cursor
//! pagination — is **bundled in the crate** (generated from Stripe's official
//! OpenAPI). So mounting Stripe needs only an API key, exactly like Jira needs
//! only its credentials — no spec to generate or paste.

use rest_connector::{AuthSpec, SourceSpec};

/// Stripe's API base URL.
pub const STRIPE_BASE_URL: &str = "https://api.stripe.com";

/// The bundled Stripe spec, generated from Stripe's official OpenAPI document.
const STRIPE_SPEC_JSON: &str = include_str!("../stripe_spec.json");

/// Build the Stripe source spec, authenticated with `api_key` (a Stripe secret
/// key). `base_url` overrides the default endpoint — useful for Stripe-compatible
/// mocks or a proxy.
pub fn stripe_spec(api_key: &str, base_url: Option<&str>) -> SourceSpec {
    let mut spec: SourceSpec =
        serde_json::from_str(STRIPE_SPEC_JSON).expect("bundled Stripe spec is valid JSON");
    spec.name = "stripe".to_string();
    spec.auth = AuthSpec::Bearer {
        token: api_key.to_string(),
    };
    if let Some(url) = base_url {
        spec.base_url = url.to_string();
    }
    spec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_spec_exposes_core_tables_and_applies_the_key() {
        let spec = stripe_spec("sk_test_x", None);
        assert_eq!(spec.base_url, STRIPE_BASE_URL);
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        for table in ["customers", "charges", "invoices", "subscriptions"] {
            assert!(spec.table(table).is_some(), "missing {table}");
        }
        // Cursor pagination came through from the import.
        assert!(!spec.tables.is_empty());
    }

    #[test]
    fn base_url_can_be_overridden() {
        let spec = stripe_spec("k", Some("http://127.0.0.1:8099"));
        assert_eq!(spec.base_url, "http://127.0.0.1:8099");
    }
}
