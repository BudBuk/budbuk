//! # activecampaign-connector
//!
//! The ActiveCampaign connector for BudBuk. Like the other REST connectors, it
//! is mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a
//! few ActiveCampaign REST (`/api/3`-style) collections and runs on the shared
//! `RestConnector` engine (caching, tracing, pushdown, FDW — all for free).
//!
//! ActiveCampaign authenticates with an `Api-Token` request header and paginates
//! its list endpoints with `?offset=N&limit=M`, wrapping the records under a
//! top-level key (e.g. `"contacts"`).

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One ActiveCampaign target. `base_url` is the account's API base (also used by
/// tests); `api_token` is sent as the `Api-Token` header on every request.
#[derive(Debug, Clone)]
pub struct ActiveCampaignConfig {
    pub base_url: String,
    pub api_token: String,
}

/// Build the ActiveCampaign source spec for a config. This is the *entire*
/// connector: endpoints, columns, pagination, and pushdown, as data.
pub fn activecampaign_spec(cfg: &ActiveCampaignConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // ActiveCampaign paginates with ?offset=N&limit=M.
    let offset = || Pagination::Offset {
        start_param: "offset".to_string(),
        limit_param: "limit".to_string(),
        page_size: 100,
    };

    SourceSpec {
        name: "activecampaign".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::ApiKeyHeader {
            header: "Api-Token".to_string(),
            value: cfg.api_token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "contacts".to_string(),
                path: "/contacts".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/contacts".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("email", "email", DataType::Text),
                    col("firstName", "firstName", DataType::Text),
                    col("lastName", "lastName", DataType::Text),
                ],
                pagination: offset(),
                filters: vec![],
            },
            TableSpec {
                name: "deals".to_string(),
                path: "/deals".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/deals".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("title", "title", DataType::Text),
                    col("value", "value", DataType::Text),
                    col("status", "status", DataType::Text),
                ],
                pagination: offset(),
                filters: vec![],
            },
            TableSpec {
                name: "lists".to_string(),
                path: "/lists".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/lists".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: offset(),
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
    fn spec_exposes_expected_tables_and_api_token_auth() {
        let cfg = ActiveCampaignConfig {
            base_url: "https://acme.api-us1.com".to_string(),
            api_token: "secret".to_string(),
        };
        let spec = activecampaign_spec(&cfg);
        assert_eq!(spec.name, "activecampaign");
        assert_eq!(spec.base_url, "https://acme.api-us1.com");
        assert!(matches!(
            spec.auth,
            AuthSpec::ApiKeyHeader { ref header, .. } if header == "Api-Token"
        ));
        assert!(spec.table("contacts").is_some());
        assert!(spec.table("deals").is_some());
        assert!(spec.table("lists").is_some());
        assert_eq!(spec.table("contacts").unwrap().columns.len(), 4);
        assert_eq!(spec.table("deals").unwrap().columns.len(), 4);
        assert_eq!(spec.table("lists").unwrap().columns.len(), 2);
        // Every table uses offset pagination with no filters.
        for name in ["contacts", "deals", "lists"] {
            let t = spec.table(name).unwrap();
            assert!(matches!(
                t.pagination,
                Pagination::Offset { page_size: 100, .. }
            ));
            assert!(t.filters.is_empty());
            assert!(
                matches!(t.row_path, RowPath::Pointer { ref pointer } if pointer == &format!("/{name}"))
            );
        }
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_contacts_from_mock() {
        let server = MockServer::start().await;
        // Return fewer rows than page_size (100) so offset pagination stops.
        Mock::given(method("GET"))
            .and(path("/contacts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "contacts": [
                    {"id": "1", "email": "a@x.com", "firstName": "A", "lastName": "One"},
                    {"id": "2", "email": "b@x.com", "firstName": "B", "lastName": "Two"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = ActiveCampaignConfig {
            base_url: server.uri(),
            api_token: "t".into(),
        };
        let rows = RestConnector::new(activecampaign_spec(&cfg))
            .fetch("contacts", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
