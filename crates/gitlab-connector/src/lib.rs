//! # gitlab-connector
//!
//! The GitLab connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! GitLab REST (`/api/v4`) collections and runs on the shared `RestConnector`
//! engine (caching, tracing, pushdown, FDW — all for free).
//!
//! It works unauthenticated against public data, or with a personal access
//! token (bearer) for private/authenticated access.

use connector_sdk::DataType;
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec,
};

/// One GitLab target. `base_url` supports self-managed instances and tests;
/// `token` (optional) authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct GitLabConfig {
    pub base_url: String,
    pub token: Option<String>,
}

/// Build the GitLab source spec for a config. This is the *entire* connector:
/// endpoints, columns, pagination, and pushdown, as data.
pub fn gitlab_spec(cfg: &GitLabConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // GitLab paginates with ?page=N&per_page=M (1-based).
    let page = || Pagination::Page {
        page_param: "page".to_string(),
        size_param: "per_page".to_string(),
        page_size: 20,
        start_page: 1,
    };
    let auth = match &cfg.token {
        Some(token) => AuthSpec::Bearer {
            token: token.clone(),
        },
        None => AuthSpec::None,
    };

    SourceSpec {
        name: "gitlab".to_string(),
        base_url: cfg.base_url.clone(),
        auth,
        tables: vec![
            TableSpec {
                name: "projects".to_string(),
                path: "/api/v4/projects".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("name", "name", DataType::Text),
                    col("path_with_namespace", "path_with_namespace", DataType::Text),
                    col("visibility", "visibility", DataType::Text),
                    col("stars", "star_count", DataType::Integer),
                    col("forks", "forks_count", DataType::Integer),
                    col("created_at", "created_at", DataType::Timestamp),
                    col("last_activity_at", "last_activity_at", DataType::Timestamp),
                ],
                pagination: page(),
                filters: vec![FilterParam {
                    column: "visibility".to_string(),
                    param: "visibility".to_string(),
                }],
            },
            TableSpec {
                name: "issues".to_string(),
                path: "/api/v4/issues".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("iid", "iid", DataType::Integer),
                    col("title", "title", DataType::Text),
                    col("state", "state", DataType::Text),
                    col("project_id", "project_id", DataType::Integer),
                    col("author", "author.username", DataType::Text),
                    col("created_at", "created_at", DataType::Timestamp),
                ],
                pagination: page(),
                filters: vec![FilterParam {
                    column: "state".to_string(),
                    param: "state".to_string(),
                }],
            },
            TableSpec {
                name: "users".to_string(),
                path: "/api/v4/users".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("username", "username", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("state", "state", DataType::Text),
                ],
                pagination: page(),
                filters: vec![FilterParam {
                    column: "username".to_string(),
                    param: "username".to_string(),
                }],
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
        let cfg = GitLabConfig {
            base_url: "https://gitlab.example.com".to_string(),
            token: Some("glpat_x".to_string()),
        };
        let spec = gitlab_spec(&cfg);
        assert_eq!(spec.name, "gitlab");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("projects").is_some());
        assert!(spec.table("issues").is_some());
        assert!(spec.table("users").is_some());
        assert_eq!(spec.table("projects").unwrap().columns.len(), 8);
        assert_eq!(
            spec.table("projects").unwrap().filters[0].param,
            "visibility"
        );
        assert_eq!(spec.table("issues").unwrap().filters[0].param, "state");
        assert_eq!(spec.table("users").unwrap().filters[0].param, "username");
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[test]
    fn no_token_produces_no_auth() {
        let cfg = GitLabConfig {
            base_url: "https://gitlab.example.com".to_string(),
            token: None,
        };
        assert!(matches!(gitlab_spec(&cfg).auth, AuthSpec::None));
    }

    #[tokio::test]
    async fn fetch_projects_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id":1,"name":"app","star_count":5,"path_with_namespace":"acme/app"}
            ])))
            .mount(&server)
            .await;
        let cfg = GitLabConfig {
            base_url: server.uri(),
            token: Some("t".into()),
        };
        let rows = RestConnector::new(gitlab_spec(&cfg))
            .fetch("projects", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
