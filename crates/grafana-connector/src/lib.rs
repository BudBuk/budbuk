//! # grafana-connector
//!
//! The Grafana connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Grafana HTTP API collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! It authenticates with a service-account / API token as a bearer token.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Grafana target. `base_url` points at the instance (e.g.
/// `https://grafana.example.com`); `token` authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct GrafanaConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Grafana source spec for a config. This is the *entire* connector:
/// endpoints, columns, and types, as data.
pub fn grafana_spec(cfg: &GrafanaConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "grafana".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "datasources".to_string(),
                path: "/datasources".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("name", "name", DataType::Text),
                    col("type", "type", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "folders".to_string(),
                path: "/folders".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("uid", "uid", DataType::Text),
                    col("title", "title", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "org_users".to_string(),
                path: "/org/users".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("userId", "userId", DataType::Integer),
                    col("login", "login", DataType::Text),
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
    fn spec_exposes_expected_tables_and_bearer_auth() {
        let cfg = GrafanaConfig {
            base_url: "https://grafana.example.com".to_string(),
            token: "glsa_x".to_string(),
        };
        let spec = grafana_spec(&cfg);
        assert_eq!(spec.name, "grafana");
        assert_eq!(spec.base_url, "https://grafana.example.com");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("datasources").is_some());
        assert!(spec.table("folders").is_some());
        assert!(spec.table("org_users").is_some());
        assert_eq!(spec.table("datasources").unwrap().columns.len(), 3);
        assert_eq!(spec.table("folders").unwrap().columns.len(), 3);
        assert_eq!(spec.table("org_users").unwrap().columns.len(), 3);
        assert_eq!(spec.table("datasources").unwrap().path, "/datasources");
        assert_eq!(spec.table("folders").unwrap().path, "/folders");
        assert_eq!(spec.table("org_users").unwrap().path, "/org/users");
        assert!(matches!(
            spec.table("datasources").unwrap().row_path,
            RowPath::Root
        ));
        assert!(matches!(
            spec.table("datasources").unwrap().pagination,
            Pagination::None
        ));
        assert!(spec.table("org_users").unwrap().filters.is_empty());
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_datasources_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/datasources"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id":1,"name":"Prometheus","type":"prometheus"}
            ])))
            .mount(&server)
            .await;
        let cfg = GrafanaConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(grafana_spec(&cfg))
            .fetch("datasources", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
