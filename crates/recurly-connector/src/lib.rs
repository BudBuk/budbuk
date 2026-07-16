//! # recurly-connector
//!
//! The Recurly connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Recurly REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! Recurly authenticates with HTTP Basic auth, using the API key as the
//! username and an empty password.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Recurly target. `base_url` supports the regional API host and tests;
/// `api_key` is sent as the HTTP Basic username (with an empty password).
#[derive(Debug, Clone)]
pub struct RecurlyConfig {
    pub base_url: String,
    pub api_key: String,
}

/// Build the Recurly source spec for a config. This is the *entire* connector:
/// endpoints, columns, and auth, as data.
pub fn recurly_spec(cfg: &RecurlyConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Recurly wraps list results in a top-level `data` array.
    let data_rows = || RowPath::Pointer {
        pointer: "/data".to_string(),
    };

    SourceSpec {
        name: "recurly".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Basic {
            username: cfg.api_key.clone(),
            password: String::new(),
        },
        tables: vec![
            TableSpec {
                name: "accounts".to_string(),
                path: "/accounts".to_string(),
                row_path: data_rows(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("code", "code", DataType::Text),
                    col("state", "state", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "subscriptions".to_string(),
                path: "/subscriptions".to_string(),
                row_path: data_rows(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("state", "state", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "invoices".to_string(),
                path: "/invoices".to_string(),
                row_path: data_rows(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("number", "number", DataType::Text),
                    col("state", "state", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use connector_sdk::{Connector, Query};
    use rest_connector::RestConnector;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn spec_exposes_expected_tables_and_basic_auth() {
        let cfg = RecurlyConfig {
            base_url: "https://v3.recurly.com".to_string(),
            api_key: "secret_key".to_string(),
        };
        let spec = recurly_spec(&cfg);
        assert_eq!(spec.name, "recurly");
        assert_eq!(spec.base_url, "https://v3.recurly.com");
        assert!(matches!(
            &spec.auth,
            AuthSpec::Basic { username, password }
                if username == "secret_key" && password.is_empty()
        ));
        assert!(spec.table("accounts").is_some());
        assert!(spec.table("subscriptions").is_some());
        assert!(spec.table("invoices").is_some());
        assert_eq!(spec.table("accounts").unwrap().columns.len(), 3);
        assert_eq!(spec.table("subscriptions").unwrap().columns.len(), 2);
        assert_eq!(spec.table("invoices").unwrap().columns.len(), 3);
        for name in ["accounts", "subscriptions", "invoices"] {
            let t = spec.table(name).unwrap();
            assert!(t.filters.is_empty());
            assert!(matches!(t.pagination, Pagination::None));
            assert!(matches!(
                &t.row_path,
                RowPath::Pointer { pointer } if pointer == "/data"
            ));
        }
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_accounts_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/accounts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": "abc", "code": "acme", "state": "active"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = RecurlyConfig {
            base_url: server.uri(),
            api_key: "k".into(),
        };
        let rows = RestConnector::new(recurly_spec(&cfg))
            .fetch("accounts", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
