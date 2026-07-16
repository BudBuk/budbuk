//! # sentry-connector
//!
//! The Sentry connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Sentry REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! It authenticates with a Sentry auth token (bearer).

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Sentry target. `base_url` supports self-hosted instances and tests;
/// `token` authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct SentryConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Sentry source spec for a config. This is the *entire* connector:
/// endpoints and columns, as data.
pub fn sentry_spec(cfg: &SentryConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "sentry".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "projects".to_string(),
                path: "/projects/".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("slug", "slug", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("platform", "platform", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "organizations".to_string(),
                path: "/organizations/".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("slug", "slug", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "teams".to_string(),
                path: "/teams/".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("slug", "slug", DataType::Text),
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
        let cfg = SentryConfig {
            base_url: "https://sentry.example.com/api/0".to_string(),
            token: "sntrys_x".to_string(),
        };
        let spec = sentry_spec(&cfg);
        assert_eq!(spec.name, "sentry");
        assert_eq!(spec.base_url, "https://sentry.example.com/api/0");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("projects").is_some());
        assert!(spec.table("organizations").is_some());
        assert!(spec.table("teams").is_some());
        assert_eq!(spec.table("projects").unwrap().columns.len(), 4);
        assert_eq!(spec.table("organizations").unwrap().columns.len(), 3);
        assert_eq!(spec.table("teams").unwrap().columns.len(), 3);
        // All tables: RowPath::Root, no pagination, no filters.
        for name in ["projects", "organizations", "teams"] {
            let t = spec.table(name).unwrap();
            assert!(matches!(t.row_path, RowPath::Root));
            assert!(matches!(t.pagination, Pagination::None));
            assert!(t.filters.is_empty());
        }
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_projects_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/projects/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id":"1","slug":"app","name":"App","platform":"python"}
            ])))
            .mount(&server)
            .await;
        let cfg = SentryConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(sentry_spec(&cfg))
            .fetch("projects", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
