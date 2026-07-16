//! # greenhouse-connector
//!
//! The Greenhouse connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Greenhouse Harvest API collections and runs on the shared `RestConnector`
//! engine (caching, tracing, pushdown, FDW — all for free).
//!
//! Greenhouse authenticates with HTTP Basic auth, using the API key as the
//! username and an empty password.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Greenhouse target. `base_url` supports test servers; `api_key`
/// authenticates via HTTP Basic auth (as the username, empty password).
#[derive(Debug, Clone)]
pub struct GreenhouseConfig {
    pub base_url: String,
    pub api_key: String,
}

/// Build the Greenhouse source spec for a config. This is the *entire*
/// connector: endpoints, columns, and pagination, as data.
pub fn greenhouse_spec(cfg: &GreenhouseConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Greenhouse paginates with ?page=N&per_page=M (1-based).
    let page = || Pagination::Page {
        page_param: "page".to_string(),
        size_param: "per_page".to_string(),
        page_size: 100,
        start_page: 1,
    };

    SourceSpec {
        name: "greenhouse".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Basic {
            username: cfg.api_key.clone(),
            password: String::new(),
        },
        tables: vec![
            TableSpec {
                name: "candidates".to_string(),
                path: "/candidates".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("first_name", "first_name", DataType::Text),
                    col("last_name", "last_name", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "jobs".to_string(),
                path: "/jobs".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("name", "name", DataType::Text),
                    col("status", "status", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "applications".to_string(),
                path: "/applications".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("status", "status", DataType::Text),
                ],
                pagination: page(),
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
        let cfg = GreenhouseConfig {
            base_url: "https://harvest.greenhouse.io".to_string(),
            api_key: "gh_key".to_string(),
        };
        let spec = greenhouse_spec(&cfg);
        assert_eq!(spec.name, "greenhouse");
        assert_eq!(spec.base_url, "https://harvest.greenhouse.io");
        assert!(matches!(
            spec.auth,
            AuthSpec::Basic { ref username, ref password }
                if username == "gh_key" && password.is_empty()
        ));
        assert!(spec.table("candidates").is_some());
        assert!(spec.table("jobs").is_some());
        assert!(spec.table("applications").is_some());
        assert_eq!(spec.table("candidates").unwrap().columns.len(), 3);
        assert_eq!(spec.table("jobs").unwrap().columns.len(), 3);
        assert_eq!(spec.table("applications").unwrap().columns.len(), 2);
        assert!(spec.table("candidates").unwrap().filters.is_empty());
        assert!(matches!(
            spec.table("candidates").unwrap().pagination,
            Pagination::Page {
                page_size: 100,
                start_page: 1,
                ..
            }
        ));
        assert!(matches!(
            spec.table("candidates").unwrap().row_path,
            RowPath::Root
        ));
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_candidates_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/candidates"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id":1,"first_name":"Ada","last_name":"Lovelace"},
                {"id":2,"first_name":"Alan","last_name":"Turing"}
            ])))
            .mount(&server)
            .await;
        let cfg = GreenhouseConfig {
            base_url: server.uri(),
            api_key: "k".into(),
        };
        let rows = RestConnector::new(greenhouse_spec(&cfg))
            .fetch("candidates", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
