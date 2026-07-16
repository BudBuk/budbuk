//! # sendgrid-connector
//!
//! The SendGrid connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! SendGrid REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! It authenticates with a SendGrid API key as a bearer token.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One SendGrid target. `base_url` supports test servers; `api_key`
/// authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct SendgridConfig {
    pub base_url: String,
    pub api_key: String,
}

/// Build the SendGrid source spec for a config. This is the *entire* connector:
/// endpoints, columns, and row paths, as data.
pub fn sendgrid_spec(cfg: &SendgridConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "sendgrid".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.api_key.clone(),
        },
        tables: vec![
            TableSpec {
                name: "templates".to_string(),
                path: "/templates".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/result".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "marketing_lists".to_string(),
                path: "/marketing/lists".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/result".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("contact_count", "contact_count", DataType::Integer),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "bounces".to_string(),
                path: "/suppression/bounces".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("email", "email", DataType::Text),
                    col("reason", "reason", DataType::Text),
                    col("created", "created", DataType::Integer),
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

    fn cfg(base_url: String) -> SendgridConfig {
        SendgridConfig {
            base_url,
            api_key: "SG.key".to_string(),
        }
    }

    #[test]
    fn spec_exposes_expected_tables_and_bearer_auth() {
        let cfg = cfg("https://api.sendgrid.com/v3".to_string());
        let spec = sendgrid_spec(&cfg);
        assert_eq!(spec.name, "sendgrid");
        assert_eq!(spec.base_url, "https://api.sendgrid.com/v3");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));

        let templates = spec.table("templates").unwrap();
        assert_eq!(templates.path, "/templates");
        assert!(matches!(
            &templates.row_path,
            RowPath::Pointer { pointer } if pointer == "/result"
        ));
        assert_eq!(templates.columns.len(), 2);
        assert!(matches!(templates.pagination, Pagination::None));
        assert!(templates.filters.is_empty());

        let lists = spec.table("marketing_lists").unwrap();
        assert_eq!(lists.path, "/marketing/lists");
        assert!(matches!(
            &lists.row_path,
            RowPath::Pointer { pointer } if pointer == "/result"
        ));
        assert_eq!(lists.columns.len(), 3);
        assert!(matches!(lists.columns[2].data_type, DataType::Integer));

        let bounces = spec.table("bounces").unwrap();
        assert_eq!(bounces.path, "/suppression/bounces");
        assert!(matches!(bounces.row_path, RowPath::Root));
        assert_eq!(bounces.columns.len(), 3);
        assert!(matches!(bounces.pagination, Pagination::None));
        assert!(bounces.filters.is_empty());

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_marketing_lists_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/marketing/lists"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": [
                    {"id": "a1", "name": "Newsletter", "contact_count": 42}
                ]
            })))
            .mount(&server)
            .await;
        let rows = RestConnector::new(sendgrid_spec(&cfg(server.uri())))
            .fetch("marketing_lists", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn fetch_bounces_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/suppression/bounces"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"email": "a@b.com", "reason": "550 blocked", "created": 1700000000}
            ])))
            .mount(&server)
            .await;
        let rows = RestConnector::new(sendgrid_spec(&cfg(server.uri())))
            .fetch("bounces", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
