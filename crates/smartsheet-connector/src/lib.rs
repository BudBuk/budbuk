//! # smartsheet-connector
//!
//! The Smartsheet connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Smartsheet REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! Smartsheet authenticates with a bearer token and wraps list responses in a
//! `{ "data": [ ... ] }` envelope, paginated with `?page=N&pageSize=M`.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Smartsheet target. `base_url` supports test servers and the regional
/// API hosts; `token` authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct SmartsheetConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Smartsheet source spec for a config. This is the *entire*
/// connector: endpoints, columns, and pagination, as data.
pub fn smartsheet_spec(cfg: &SmartsheetConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Smartsheet paginates with ?page=N&pageSize=M (1-based) and wraps rows in
    // a `data` array.
    let page = || Pagination::Page {
        page_param: "page".to_string(),
        size_param: "pageSize".to_string(),
        page_size: 100,
        start_page: 1,
    };
    let data = || RowPath::Pointer {
        pointer: "/data".to_string(),
    };
    let id_name = |name: &str, path: &str| TableSpec {
        name: name.to_string(),
        path: path.to_string(),
        row_path: data(),
        columns: vec![
            col("id", "id", DataType::Integer),
            col("name", "name", DataType::Text),
        ],
        pagination: page(),
        filters: vec![],
    };

    SourceSpec {
        name: "smartsheet".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            id_name("sheets", "/sheets"),
            id_name("reports", "/reports"),
            id_name("workspaces", "/workspaces"),
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
        let cfg = SmartsheetConfig {
            base_url: "https://api.smartsheet.com/2.0".to_string(),
            token: "ss_token".to_string(),
        };
        let spec = smartsheet_spec(&cfg);
        assert_eq!(spec.name, "smartsheet");
        assert_eq!(spec.base_url, "https://api.smartsheet.com/2.0");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        for name in ["sheets", "reports", "workspaces"] {
            let t = spec.table(name).unwrap();
            assert_eq!(t.columns.len(), 2);
            assert!(t.filters.is_empty());
            assert!(matches!(&t.row_path, RowPath::Pointer { pointer } if pointer == "/data"));
            assert!(matches!(
                t.pagination,
                Pagination::Page {
                    page_size: 100,
                    start_page: 1,
                    ..
                }
            ));
        }
        assert_eq!(spec.table("sheets").unwrap().path, "/sheets");
        assert_eq!(spec.table("reports").unwrap().path, "/reports");
        assert_eq!(spec.table("workspaces").unwrap().path, "/workspaces");
        assert!(spec.table("missing").is_none());
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_sheets_from_mock() {
        let server = MockServer::start().await;
        // Fewer rows than page_size (100) so Page pagination terminates.
        Mock::given(method("GET"))
            .and(path("/sheets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": 1, "name": "Q3 Plan"},
                    {"id": 2, "name": "Roadmap"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = SmartsheetConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(smartsheet_spec(&cfg))
            .fetch("sheets", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
