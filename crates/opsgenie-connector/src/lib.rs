//! # opsgenie-connector
//!
//! The Opsgenie connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Opsgenie REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! Opsgenie authenticates with an API key sent as
//! `Authorization: GenieKey <key>`, and paginates with `?offset=N&limit=M`.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Opsgenie target. `base_url` supports regional endpoints and tests;
/// `api_key` authenticates as a `GenieKey`.
#[derive(Debug, Clone)]
pub struct OpsgenieConfig {
    pub base_url: String,
    pub api_key: String,
}

/// Build the Opsgenie source spec for a config. This is the *entire* connector:
/// endpoints, columns, and pagination, as data.
pub fn opsgenie_spec(cfg: &OpsgenieConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Opsgenie wraps records in `{ "data": [ ... ] }` and paginates with
    // ?offset=N&limit=M.
    let row_path = || RowPath::Pointer {
        pointer: "/data".to_string(),
    };
    let page = || Pagination::Offset {
        start_param: "offset".to_string(),
        limit_param: "limit".to_string(),
        page_size: 100,
    };

    SourceSpec {
        name: "opsgenie".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::ApiKeyHeader {
            header: "Authorization".to_string(),
            value: format!("GenieKey {}", cfg.api_key),
        },
        tables: vec![
            TableSpec {
                name: "alerts".to_string(),
                path: "/alerts".to_string(),
                row_path: row_path(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("message", "message", DataType::Text),
                    col("status", "status", DataType::Text),
                    col("priority", "priority", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "teams".to_string(),
                path: "/teams".to_string(),
                row_path: row_path(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "schedules".to_string(),
                path: "/schedules".to_string(),
                row_path: row_path(),
                columns: vec![
                    col("id", "id", DataType::Text),
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
    use connector_sdk::{Connector, Query};
    use rest_connector::RestConnector;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn spec_exposes_expected_tables_and_geniekey_auth() {
        let cfg = OpsgenieConfig {
            base_url: "https://api.opsgenie.com/v2".to_string(),
            api_key: "secret".to_string(),
        };
        let spec = opsgenie_spec(&cfg);
        assert_eq!(spec.name, "opsgenie");
        assert_eq!(spec.base_url, "https://api.opsgenie.com/v2");
        assert!(matches!(&spec.auth, AuthSpec::ApiKeyHeader { .. }));
        // Assert the header name and that the value formats as `GenieKey <key>`
        // without a fallible match arm (which would leave an uncovered region).
        let auth_json = serde_json::to_value(&spec.auth).unwrap();
        assert_eq!(auth_json["header"], "Authorization");
        assert_eq!(auth_json["value"], "GenieKey secret");
        assert!(spec.table("alerts").is_some());
        assert!(spec.table("teams").is_some());
        assert!(spec.table("schedules").is_some());
        assert_eq!(spec.table("alerts").unwrap().columns.len(), 4);
        assert_eq!(spec.table("teams").unwrap().columns.len(), 2);
        assert_eq!(spec.table("schedules").unwrap().columns.len(), 2);
        assert!(spec.table("alerts").unwrap().filters.is_empty());
        assert!(matches!(
            spec.table("alerts").unwrap().row_path,
            RowPath::Pointer { .. }
        ));
        assert!(matches!(
            spec.table("teams").unwrap().pagination,
            Pagination::Offset { .. }
        ));
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_alerts_from_mock() {
        let server = MockServer::start().await;
        // Return fewer rows than page_size (100) so offset pagination
        // terminates after one request.
        Mock::given(method("GET"))
            .and(path("/alerts"))
            .and(header("Authorization", "GenieKey k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id":"a1","message":"disk full","status":"open","priority":"P1"},
                    {"id":"a2","message":"cpu high","status":"closed","priority":"P3"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = OpsgenieConfig {
            base_url: server.uri(),
            api_key: "k".into(),
        };
        let connector = RestConnector::new(opsgenie_spec(&cfg));
        assert_eq!(connector.name(), "opsgenie");
        let schemas = connector.discover().await.unwrap();
        assert_eq!(schemas.len(), 3);
        let rows = connector.fetch("alerts", &Query::default()).await.unwrap();
        assert_eq!(rows.len(), 2);
    }
}
