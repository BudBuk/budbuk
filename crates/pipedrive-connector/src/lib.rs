//! # pipedrive-connector
//!
//! The Pipedrive connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Pipedrive REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! Pipedrive authenticates with an `?api_token=<token>` query parameter and
//! wraps its record arrays in a `{"data": [...]}` envelope, paginated with
//! `?start=<offset>&limit=<n>`.

use connector_sdk::DataType;
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec,
};

/// One Pipedrive target. `base_url` supports the region-specific hosts and
/// tests; `api_token` authenticates via the `api_token` query parameter.
#[derive(Debug, Clone)]
pub struct PipedriveConfig {
    pub base_url: String,
    pub api_token: String,
}

/// Build the Pipedrive source spec for a config. This is the *entire*
/// connector: endpoints, columns, pagination, and pushdown, as data.
pub fn pipedrive_spec(cfg: &PipedriveConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Pipedrive wraps records in {"data": [...]}.
    let row_path = || RowPath::Pointer {
        pointer: "/data".to_string(),
    };
    // Pipedrive paginates with ?start=<offset>&limit=<n>.
    let page = || Pagination::Offset {
        start_param: "start".to_string(),
        limit_param: "limit".to_string(),
        page_size: 100,
    };

    SourceSpec {
        name: "pipedrive".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::ApiKeyQuery {
            param: "api_token".to_string(),
            value: cfg.api_token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "deals".to_string(),
                path: "/deals".to_string(),
                row_path: row_path(),
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("title", "title", DataType::Text),
                    col("status", "status", DataType::Text),
                    col("value", "value", DataType::Float),
                ],
                pagination: page(),
                filters: vec![FilterParam {
                    column: "status".to_string(),
                    param: "status".to_string(),
                }],
            },
            TableSpec {
                name: "persons".to_string(),
                path: "/persons".to_string(),
                row_path: row_path(),
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("name", "name", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "organizations".to_string(),
                path: "/organizations".to_string(),
                row_path: row_path(),
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("name", "name", DataType::Text),
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
    use connector_sdk::{Connector, Filter, Operator, Query, Value};
    use rest_connector::RestConnector;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn spec_exposes_expected_tables_and_query_auth() {
        let cfg = PipedriveConfig {
            base_url: "https://acme.pipedrive.com/api/v1".to_string(),
            api_token: "tok_x".to_string(),
        };
        let spec = pipedrive_spec(&cfg);
        assert_eq!(spec.name, "pipedrive");
        assert_eq!(spec.base_url, "https://acme.pipedrive.com/api/v1");
        assert!(matches!(spec.auth, AuthSpec::ApiKeyQuery { .. }));
        // All three tables present with the expected column counts.
        assert_eq!(spec.table("deals").unwrap().columns.len(), 4);
        assert_eq!(spec.table("persons").unwrap().columns.len(), 2);
        assert_eq!(spec.table("organizations").unwrap().columns.len(), 2);
        assert!(spec.table("missing").is_none());
        // Deals carries a single status filter; the others carry none.
        assert_eq!(spec.table("deals").unwrap().filters[0].param, "status");
        assert!(spec.table("persons").unwrap().filters.is_empty());
        assert!(spec.table("organizations").unwrap().filters.is_empty());
        // Every table uses the /data envelope and offset pagination.
        for name in ["deals", "persons", "organizations"] {
            let t = spec.table(name).unwrap();
            assert!(matches!(t.row_path, RowPath::Pointer { .. }));
            assert!(matches!(t.pagination, Pagination::Offset { .. }));
        }
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_deals_with_status_pushdown_from_mock() {
        let server = MockServer::start().await;
        // Return fewer rows than page_size so offset pagination terminates,
        // and require the api_token + pushed-down status query params.
        Mock::given(method("GET"))
            .and(path("/deals"))
            .and(query_param("api_token", "tok"))
            .and(query_param("status", "won"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": 1, "title": "Big deal", "status": "won", "value": 1000.5},
                    {"id": 2, "title": "Small deal", "status": "won", "value": 42.0}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = PipedriveConfig {
            base_url: server.uri(),
            api_token: "tok".to_string(),
        };
        let query = Query {
            filters: vec![Filter::new(
                "status",
                Operator::Eq,
                Value::Text("won".to_string()),
            )],
            ..Default::default()
        };
        let rows = RestConnector::new(pipedrive_spec(&cfg))
            .fetch("deals", &query)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
