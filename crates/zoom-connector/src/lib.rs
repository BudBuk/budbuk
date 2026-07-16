//! # zoom-connector
//!
//! The Zoom connector for BudBuk. Like the other REST connectors, it is mostly
//! *config*: it builds a [`rest_connector::SourceSpec`] describing a few Zoom
//! REST collections and runs on the shared `RestConnector` engine (caching,
//! tracing, pushdown, FDW — all for free).
//!
//! Zoom requires an OAuth bearer token for every request.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Zoom target. `base_url` supports tests and regional endpoints; `token`
/// authenticates as an OAuth bearer token.
#[derive(Debug, Clone)]
pub struct ZoomConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Zoom source spec for a config. This is the *entire* connector:
/// endpoints, columns, and row extraction, as data.
pub fn zoom_spec(cfg: &ZoomConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "zoom".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "users".to_string(),
                path: "/users".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/users".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("email", "email", DataType::Text),
                    col("type", "type", DataType::Integer),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "roles".to_string(),
                path: "/roles".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/roles".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "groups".to_string(),
                path: "/groups".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/groups".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
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
        let cfg = ZoomConfig {
            base_url: "https://api.zoom.us/v2".to_string(),
            token: "zoom_token".to_string(),
        };
        let spec = zoom_spec(&cfg);
        assert_eq!(spec.name, "zoom");
        assert_eq!(spec.base_url, "https://api.zoom.us/v2");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("users").is_some());
        assert!(spec.table("roles").is_some());
        assert!(spec.table("groups").is_some());
        assert_eq!(spec.table("users").unwrap().columns.len(), 3);
        assert_eq!(spec.table("roles").unwrap().columns.len(), 2);
        assert_eq!(spec.table("groups").unwrap().columns.len(), 2);
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
                "users": [
                    {"id": "abc", "email": "a@example.com", "type": 1},
                    {"id": "def", "email": "b@example.com", "type": 2}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = ZoomConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(zoom_spec(&cfg))
            .fetch("users", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
