//! # confluence-connector
//!
//! The Confluence connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Confluence Cloud REST collections and runs on the shared `RestConnector`
//! engine (caching, tracing, pushdown, FDW — all for free).
//!
//! Confluence Cloud authenticates with HTTP Basic using your Atlassian account
//! email as the username and an API token as the password.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Confluence Cloud target. `base_url` is the site's REST root (e.g.
/// `https://your-domain.atlassian.net/wiki/rest/api`); `email` + `api_token`
/// authenticate as HTTP Basic.
#[derive(Debug, Clone)]
pub struct ConfluenceConfig {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
}

/// Build the Confluence source spec for a config. This is the *entire*
/// connector: endpoints, columns, and pagination, as data.
pub fn confluence_spec(cfg: &ConfluenceConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Confluence Cloud paginates with ?start=N&limit=M (offset style), wrapping
    // the records in a top-level "results" array.
    let page = || Pagination::Offset {
        start_param: "start".to_string(),
        limit_param: "limit".to_string(),
        page_size: 25,
    };
    let results = || RowPath::Pointer {
        pointer: "/results".to_string(),
    };

    SourceSpec {
        name: "confluence".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Basic {
            username: cfg.email.clone(),
            password: cfg.api_token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "content".to_string(),
                path: "/content".to_string(),
                row_path: results(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("type", "type", DataType::Text),
                    col("title", "title", DataType::Text),
                    col("status", "status", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "space".to_string(),
                path: "/space".to_string(),
                row_path: results(),
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("key", "key", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("type", "type", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "group".to_string(),
                path: "/group".to_string(),
                row_path: results(),
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
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn config() -> ConfluenceConfig {
        ConfluenceConfig {
            base_url: "https://acme.atlassian.net/wiki/rest/api".to_string(),
            email: "user@acme.com".to_string(),
            api_token: "tok_x".to_string(),
        }
    }

    #[test]
    fn spec_exposes_expected_tables_and_basic_auth() {
        let cfg = config();
        let spec = confluence_spec(&cfg);
        assert_eq!(spec.name, "confluence");
        assert_eq!(spec.base_url, cfg.base_url);
        assert!(matches!(spec.auth, AuthSpec::Basic { .. }));

        let content = spec.table("content").unwrap();
        assert_eq!(content.columns.len(), 4);
        assert!(matches!(
            content.row_path,
            RowPath::Pointer { ref pointer } if pointer == "/results"
        ));
        assert!(matches!(
            content.pagination,
            Pagination::Offset { ref start_param, ref limit_param, page_size }
                if start_param == "start" && limit_param == "limit" && page_size == 25
        ));
        assert!(content.filters.is_empty());

        assert_eq!(spec.table("space").unwrap().columns.len(), 4);
        assert_eq!(spec.table("group").unwrap().columns.len(), 2);
        assert!(spec.table("missing").is_none());

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_content_from_mock() {
        let server = MockServer::start().await;
        // Fewer rows than page_size (25) so offset pagination terminates.
        Mock::given(method("GET"))
            .and(path("/content"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    {"id": "1", "type": "page", "title": "Home", "status": "current"},
                    {"id": "2", "type": "blogpost", "title": "News", "status": "current"}
                ]
            })))
            .mount(&server)
            .await;

        let cfg = ConfluenceConfig {
            base_url: server.uri(),
            ..config()
        };
        let rows = RestConnector::new(confluence_spec(&cfg))
            .fetch("content", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
