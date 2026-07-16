//! # bitbucket-connector
//!
//! The Bitbucket connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Bitbucket Cloud REST collections and runs on the shared `RestConnector`
//! engine (caching, tracing, pushdown, FDW — all for free).
//!
//! It authenticates with HTTP Basic auth using a username and an
//! [app password](https://support.atlassian.com/bitbucket-cloud/docs/app-passwords/).

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Bitbucket target. `base_url` supports test servers; `username` and
/// `app_password` authenticate via HTTP Basic auth.
#[derive(Debug, Clone)]
pub struct BitbucketConfig {
    pub base_url: String,
    pub username: String,
    pub app_password: String,
}

/// Build the Bitbucket source spec for a config. This is the *entire*
/// connector: endpoints, columns, and pagination, as data.
pub fn bitbucket_spec(cfg: &BitbucketConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Bitbucket Cloud paginates with ?page=N&pagelen=M (1-based) and returns
    // records under the `/values` key.
    let page = || Pagination::Page {
        page_param: "page".to_string(),
        size_param: "pagelen".to_string(),
        page_size: 50,
        start_page: 1,
    };
    let values = || RowPath::Pointer {
        pointer: "/values".to_string(),
    };

    SourceSpec {
        name: "bitbucket".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Basic {
            username: cfg.username.clone(),
            password: cfg.app_password.clone(),
        },
        tables: vec![
            TableSpec {
                name: "repositories".to_string(),
                path: "/repositories".to_string(),
                row_path: values(),
                columns: vec![
                    col("uuid", "uuid", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("full_name", "full_name", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "workspaces".to_string(),
                path: "/workspaces".to_string(),
                row_path: values(),
                columns: vec![
                    col("uuid", "uuid", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("slug", "slug", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "snippets".to_string(),
                path: "/snippets".to_string(),
                row_path: values(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("title", "title", DataType::Text),
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
        let cfg = BitbucketConfig {
            base_url: "https://api.bitbucket.org/2.0".to_string(),
            username: "alice".to_string(),
            app_password: "secret".to_string(),
        };
        let spec = bitbucket_spec(&cfg);
        assert_eq!(spec.name, "bitbucket");
        assert_eq!(spec.base_url, "https://api.bitbucket.org/2.0");
        assert!(matches!(spec.auth, AuthSpec::Basic { .. }));
        assert!(spec.table("repositories").is_some());
        assert!(spec.table("workspaces").is_some());
        assert!(spec.table("snippets").is_some());
        assert_eq!(spec.table("repositories").unwrap().columns.len(), 3);
        assert_eq!(spec.table("workspaces").unwrap().columns.len(), 3);
        assert_eq!(spec.table("snippets").unwrap().columns.len(), 2);
        // Every table paginates by page under /values with no filters.
        for name in ["repositories", "workspaces", "snippets"] {
            let t = spec.table(name).unwrap();
            assert!(matches!(t.row_path, RowPath::Pointer { .. }));
            assert!(matches!(t.pagination, Pagination::Page { .. }));
            assert!(t.filters.is_empty());
        }
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_repositories_from_mock() {
        let server = MockServer::start().await;
        // Fewer rows than page_size (50) so Page pagination terminates.
        Mock::given(method("GET"))
            .and(path("/repositories"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "values": [
                    {"uuid": "{r1}", "name": "app", "full_name": "acme/app"},
                    {"uuid": "{r2}", "name": "lib", "full_name": "acme/lib"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = BitbucketConfig {
            base_url: server.uri(),
            username: "alice".into(),
            app_password: "secret".into(),
        };
        let rows = RestConnector::new(bitbucket_spec(&cfg))
            .fetch("repositories", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
