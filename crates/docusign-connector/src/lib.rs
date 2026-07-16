//! # docusign-connector
//!
//! The DocuSign connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! DocuSign eSignature REST collections and runs on the shared `RestConnector`
//! engine (caching, tracing, pushdown, FDW — all for free).
//!
//! It authenticates with a bearer token (an OAuth access token).

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One DocuSign target. `base_url` points at an account's REST base (or a test
/// server); `token` authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct DocusignConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the DocuSign source spec for a config. This is the *entire* connector:
/// endpoints, columns, and row paths, as data.
pub fn docusign_spec(cfg: &DocusignConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "docusign".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "templates".to_string(),
                path: "/templates".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/envelopeTemplates".to_string(),
                },
                columns: vec![
                    col("templateId", "templateId", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "folders".to_string(),
                path: "/folders".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/folders".to_string(),
                },
                columns: vec![
                    col("folderId", "folderId", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "users".to_string(),
                path: "/users".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/users".to_string(),
                },
                columns: vec![
                    col("userId", "userId", DataType::Text),
                    col("userName", "userName", DataType::Text),
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
        let cfg = DocusignConfig {
            base_url: "https://demo.docusign.net".to_string(),
            token: "ds_token".to_string(),
        };
        let spec = docusign_spec(&cfg);
        assert_eq!(spec.name, "docusign");
        assert_eq!(spec.base_url, "https://demo.docusign.net");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("templates").is_some());
        assert!(spec.table("folders").is_some());
        assert!(spec.table("users").is_some());
        assert_eq!(spec.table("templates").unwrap().columns.len(), 2);
        assert_eq!(spec.table("folders").unwrap().columns.len(), 2);
        assert_eq!(spec.table("users").unwrap().columns.len(), 3);
        assert!(matches!(
            spec.table("templates").unwrap().row_path,
            RowPath::Pointer { .. }
        ));
        assert!(matches!(
            spec.table("templates").unwrap().pagination,
            Pagination::None
        ));
        assert!(spec.table("users").unwrap().filters.is_empty());
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_templates_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/templates"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "envelopeTemplates": [
                    {"templateId": "abc", "name": "NDA"},
                    {"templateId": "def", "name": "Offer"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = DocusignConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(docusign_spec(&cfg))
            .fetch("templates", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
