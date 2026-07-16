//! # typeform-connector
//!
//! The Typeform connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Typeform API collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! Typeform authenticates with a personal access token (bearer) and wraps its
//! list responses in an `{"items": [...]}` envelope, paginated with
//! `?page=N&page_size=M` (1-based).

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Typeform target. `base_url` supports tests and self-hosted proxies;
/// `token` authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct TypeformConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Typeform source spec for a config. This is the *entire* connector:
/// endpoints, columns, and pagination, as data.
pub fn typeform_spec(cfg: &TypeformConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Typeform wraps lists in {"items": [...]}.
    let items = || RowPath::Pointer {
        pointer: "/items".to_string(),
    };
    // Typeform paginates with ?page=N&page_size=M (1-based).
    let page = || Pagination::Page {
        page_param: "page".to_string(),
        size_param: "page_size".to_string(),
        page_size: 200,
        start_page: 1,
    };

    SourceSpec {
        name: "typeform".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "forms".to_string(),
                path: "/forms".to_string(),
                row_path: items(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("title", "title", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "themes".to_string(),
                path: "/themes".to_string(),
                row_path: items(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "workspaces".to_string(),
                path: "/workspaces".to_string(),
                row_path: items(),
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

    #[test]
    fn spec_exposes_expected_tables_and_bearer_auth() {
        let cfg = TypeformConfig {
            base_url: "https://api.typeform.com".to_string(),
            token: "tfp_x".to_string(),
        };
        let spec = typeform_spec(&cfg);
        assert_eq!(spec.name, "typeform");
        assert_eq!(spec.base_url, "https://api.typeform.com");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        for name in ["forms", "themes", "workspaces"] {
            let table = spec.table(name).unwrap();
            assert_eq!(table.columns.len(), 2);
            assert!(table.filters.is_empty());
            assert!(matches!(
                &table.row_path,
                RowPath::Pointer { pointer } if pointer == "/items"
            ));
            assert!(matches!(
                &table.pagination,
                Pagination::Page {
                    page_param,
                    size_param,
                    page_size: 200,
                    start_page: 1,
                } if page_param == "page" && size_param == "page_size"
            ));
        }
        assert_eq!(spec.table("forms").unwrap().path, "/forms");
        assert_eq!(spec.table("themes").unwrap().path, "/themes");
        assert_eq!(spec.table("workspaces").unwrap().path, "/workspaces");
        assert!(spec.table("missing").is_none());
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_forms_from_mock() {
        let server = MockServer::start().await;
        // Fewer rows than page_size (200), so Page pagination stops after one page.
        Mock::given(method("GET"))
            .and(path("/forms"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {"id": "abc", "title": "Survey"},
                    {"id": "def", "title": "Quiz"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = TypeformConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(typeform_spec(&cfg))
            .fetch("forms", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
