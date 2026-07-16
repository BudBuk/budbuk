//! # msgraph-connector
//!
//! The Microsoft Graph connector for BudBuk. Like the other REST connectors, it
//! is mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a
//! few Microsoft Graph (`/v1.0`) collections and runs on the shared
//! `RestConnector` engine (caching, tracing, pushdown, FDW — all for free).
//!
//! Microsoft Graph wraps its collections in a top-level `{"value": [...]}`
//! envelope, so every table reads its rows from the `/value` JSON pointer and
//! authenticates with an OAuth bearer token.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Microsoft Graph target. `base_url` supports test servers and sovereign
/// clouds; `token` is the OAuth 2.0 access token sent as a bearer credential.
#[derive(Debug, Clone)]
pub struct MsGraphConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Microsoft Graph source spec for a config. This is the *entire*
/// connector: endpoints, columns, and row envelope, as data.
pub fn msgraph_spec(cfg: &MsGraphConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Graph nests collections under a top-level "value" array.
    let envelope = || RowPath::Pointer {
        pointer: "/value".to_string(),
    };

    SourceSpec {
        name: "msgraph".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "users".to_string(),
                path: "/users".to_string(),
                row_path: envelope(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("displayName", "displayName", DataType::Text),
                    col("mail", "mail", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "groups".to_string(),
                path: "/groups".to_string(),
                row_path: envelope(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("displayName", "displayName", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "applications".to_string(),
                path: "/applications".to_string(),
                row_path: envelope(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("displayName", "displayName", DataType::Text),
                    col("appId", "appId", DataType::Text),
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
        let cfg = MsGraphConfig {
            base_url: "https://graph.microsoft.com/v1.0".to_string(),
            token: "aad_token".to_string(),
        };
        let spec = msgraph_spec(&cfg);
        assert_eq!(spec.name, "msgraph");
        assert_eq!(spec.base_url, "https://graph.microsoft.com/v1.0");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("users").is_some());
        assert!(spec.table("groups").is_some());
        assert!(spec.table("applications").is_some());
        assert_eq!(spec.table("users").unwrap().columns.len(), 3);
        assert_eq!(spec.table("groups").unwrap().columns.len(), 2);
        assert_eq!(spec.table("applications").unwrap().columns.len(), 3);
        // Every table reads rows from the Graph "value" envelope with no paging.
        for name in ["users", "groups", "applications"] {
            let t = spec.table(name).unwrap();
            assert!(matches!(&t.row_path, RowPath::Pointer { pointer } if pointer == "/value"));
            assert!(matches!(t.pagination, Pagination::None));
            assert!(t.filters.is_empty());
        }
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
                "value": [
                    {"id":"1","displayName":"Ada Lovelace","mail":"ada@example.com"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = MsGraphConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(msgraph_spec(&cfg))
            .fetch("users", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
