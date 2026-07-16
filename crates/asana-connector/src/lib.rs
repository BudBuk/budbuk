//! # asana-connector
//!
//! The Asana connector for BudBuk. Like the other REST connectors, it is mostly
//! *config*: it builds a [`rest_connector::SourceSpec`] describing a few Asana
//! REST collections and runs on the shared `RestConnector` engine (caching,
//! tracing, pushdown, FDW — all for free).
//!
//! Asana wraps its list responses in a `{"data": [...]}` envelope, so every
//! table reads its rows from the `/data` JSON pointer. It works with a personal
//! access token (bearer), or unauthenticated against a mock/test server.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Asana target. `base_url` supports tests and alternate hosts; `token`
/// (optional) authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct AsanaConfig {
    pub base_url: String,
    pub token: Option<String>,
}

/// Build the Asana source spec for a config. This is the *entire* connector:
/// endpoints and columns, as data.
pub fn asana_spec(cfg: &AsanaConfig) -> SourceSpec {
    let col = |name: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: name.to_string(),
        data_type,
    };
    let auth = match &cfg.token {
        Some(token) => AuthSpec::Bearer {
            token: token.clone(),
        },
        None => AuthSpec::None,
    };

    SourceSpec {
        name: "asana".to_string(),
        base_url: cfg.base_url.clone(),
        auth,
        tables: vec![
            TableSpec {
                name: "projects".to_string(),
                path: "/projects".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/data".to_string(),
                },
                columns: vec![
                    col("gid", DataType::Text),
                    col("name", DataType::Text),
                    col("archived", DataType::Bool),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "tasks".to_string(),
                path: "/tasks".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/data".to_string(),
                },
                columns: vec![
                    col("gid", DataType::Text),
                    col("name", DataType::Text),
                    col("completed", DataType::Bool),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "users".to_string(),
                path: "/users".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/data".to_string(),
                },
                columns: vec![
                    col("gid", DataType::Text),
                    col("name", DataType::Text),
                    col("email", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "workspaces".to_string(),
                path: "/workspaces".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/data".to_string(),
                },
                columns: vec![col("gid", DataType::Text), col("name", DataType::Text)],
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
        let cfg = AsanaConfig {
            base_url: "https://app.asana.com/api/1.0".to_string(),
            token: Some("1/abc".to_string()),
        };
        let spec = asana_spec(&cfg);
        assert_eq!(spec.name, "asana");
        assert_eq!(spec.base_url, "https://app.asana.com/api/1.0");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("projects").is_some());
        assert!(spec.table("tasks").is_some());
        assert!(spec.table("users").is_some());
        assert!(spec.table("workspaces").is_some());
        assert_eq!(spec.table("projects").unwrap().columns.len(), 3);
        assert_eq!(spec.table("tasks").unwrap().columns.len(), 3);
        assert_eq!(spec.table("users").unwrap().columns.len(), 3);
        assert_eq!(spec.table("workspaces").unwrap().columns.len(), 2);
        // Every table reads rows from the /data envelope with no pagination.
        for name in ["projects", "tasks", "users", "workspaces"] {
            let t = spec.table(name).unwrap();
            assert!(matches!(&t.row_path, RowPath::Pointer { pointer } if pointer == "/data"));
            assert!(matches!(t.pagination, Pagination::None));
            assert!(t.filters.is_empty());
        }
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[test]
    fn no_token_produces_no_auth() {
        let cfg = AsanaConfig {
            base_url: "https://app.asana.com/api/1.0".to_string(),
            token: None,
        };
        assert!(matches!(asana_spec(&cfg).auth, AuthSpec::None));
    }

    #[tokio::test]
    async fn fetch_projects_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/projects"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"gid": "1", "name": "Launch", "archived": false}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = AsanaConfig {
            base_url: server.uri(),
            token: Some("1/abc".into()),
        };
        let rows = RestConnector::new(asana_spec(&cfg))
            .fetch("projects", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
