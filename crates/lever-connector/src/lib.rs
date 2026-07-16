//! # lever-connector
//!
//! The Lever connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Lever REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! Lever authenticates with HTTP Basic auth, using the API key as the username
//! and an empty password. Its list endpoints wrap records in a `{"data": [...]}`
//! envelope, so every table reads rows from the `/data` JSON pointer.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Lever target. `base_url` supports tests and future overrides; `api_key`
/// authenticates as the HTTP Basic username (with an empty password).
#[derive(Debug, Clone)]
pub struct LeverConfig {
    pub base_url: String,
    pub api_key: String,
}

/// Build the Lever source spec for a config. This is the *entire* connector:
/// endpoints, columns, and row extraction, as data.
pub fn lever_spec(cfg: &LeverConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Every Lever list endpoint wraps records in a `{"data": [...]}` envelope.
    let data = || RowPath::Pointer {
        pointer: "/data".to_string(),
    };

    SourceSpec {
        name: "lever".to_string(),
        base_url: cfg.base_url.clone(),
        // Lever uses the API key as the Basic username with an empty password.
        auth: AuthSpec::Basic {
            username: cfg.api_key.clone(),
            password: String::new(),
        },
        tables: vec![
            TableSpec {
                name: "opportunities".to_string(),
                path: "/opportunities".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "postings".to_string(),
                path: "/postings".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("text", "text", DataType::Text),
                    col("state", "state", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "users".to_string(),
                path: "/users".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("email", "email", DataType::Text),
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
        let cfg = LeverConfig {
            base_url: "https://api.lever.co/v1".to_string(),
            api_key: "key_x".to_string(),
        };
        let spec = lever_spec(&cfg);
        assert_eq!(spec.name, "lever");
        assert_eq!(spec.base_url, "https://api.lever.co/v1");
        assert!(matches!(
            spec.auth,
            AuthSpec::Basic { ref username, ref password }
                if username == "key_x" && password.is_empty()
        ));
        assert!(spec.table("opportunities").is_some());
        assert!(spec.table("postings").is_some());
        assert!(spec.table("users").is_some());
        assert_eq!(spec.table("opportunities").unwrap().columns.len(), 2);
        assert_eq!(spec.table("postings").unwrap().columns.len(), 3);
        assert_eq!(spec.table("users").unwrap().columns.len(), 3);
        for name in ["opportunities", "postings", "users"] {
            let t = spec.table(name).unwrap();
            assert!(matches!(t.row_path, RowPath::Pointer { ref pointer } if pointer == "/data"));
            assert!(matches!(t.pagination, Pagination::None));
            assert!(t.filters.is_empty());
        }
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_opportunities_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/opportunities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": "op1", "name": "Jane Doe"},
                    {"id": "op2", "name": "John Roe"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = LeverConfig {
            base_url: server.uri(),
            api_key: "k".into(),
        };
        let rows = RestConnector::new(lever_spec(&cfg))
            .fetch("opportunities", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
