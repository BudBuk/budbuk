//! # box-connector
//!
//! The Box connector for BudBuk. Like the other REST connectors, it is mostly
//! *config*: it builds a [`rest_connector::SourceSpec`] describing a few Box
//! REST collections and runs on the shared `RestConnector` engine (caching,
//! tracing, pushdown, FDW — all for free).
//!
//! Box authenticates with a bearer token.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Box target. `base_url` supports tests and regional endpoints; `token`
/// authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct BoxConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Box source spec for a config. This is the *entire* connector:
/// endpoints, columns, pagination, and pushdown, as data.
pub fn box_spec(cfg: &BoxConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Box wraps its collections in an `entries` array.
    let entries = || RowPath::Pointer {
        pointer: "/entries".to_string(),
    };

    SourceSpec {
        name: "box".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "users".to_string(),
                path: "/users".to_string(),
                row_path: entries(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("login", "login", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "groups".to_string(),
                path: "/groups".to_string(),
                row_path: entries(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "webhooks".to_string(),
                path: "/webhooks".to_string(),
                row_path: entries(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("address", "address", DataType::Text),
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
    fn spec_exposes_expected_tables_and_bearer_auth() {
        let cfg = BoxConfig {
            base_url: "https://api.box.com/2.0".to_string(),
            token: "box_token".to_string(),
        };
        let spec = box_spec(&cfg);
        assert_eq!(spec.name, "box");
        assert_eq!(spec.base_url, "https://api.box.com/2.0");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("users").is_some());
        assert!(spec.table("groups").is_some());
        assert!(spec.table("webhooks").is_some());
        assert_eq!(spec.table("users").unwrap().columns.len(), 3);
        assert_eq!(spec.table("groups").unwrap().columns.len(), 2);
        assert_eq!(spec.table("webhooks").unwrap().columns.len(), 2);
        assert!(matches!(
            spec.table("users").unwrap().row_path,
            RowPath::Pointer { .. }
        ));
        assert!(matches!(
            spec.table("users").unwrap().pagination,
            Pagination::None
        ));
        assert!(spec.table("users").unwrap().filters.is_empty());
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_users_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "entries": [
                    {"id":"1","name":"Ada","login":"ada@example.com"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = BoxConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(box_spec(&cfg))
            .fetch("users", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
