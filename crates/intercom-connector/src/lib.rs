//! # intercom-connector
//!
//! The Intercom connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Intercom REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! It authenticates with an access token (bearer).

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Intercom target. `base_url` supports regional hosts and tests; `token`
/// authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct IntercomConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Intercom source spec for a config. This is the *entire* connector:
/// endpoints, columns, and row paths, as data.
pub fn intercom_spec(cfg: &IntercomConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "intercom".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "contacts".to_string(),
                path: "/contacts".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/data".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("email", "email", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "companies".to_string(),
                path: "/companies".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/data".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("company_id", "company_id", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "admins".to_string(),
                path: "/admins".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/admins".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
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
        let cfg = IntercomConfig {
            base_url: "https://api.intercom.io".to_string(),
            token: "tok_x".to_string(),
        };
        let spec = intercom_spec(&cfg);
        assert_eq!(spec.name, "intercom");
        assert_eq!(spec.base_url, "https://api.intercom.io");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));

        let contacts = spec.table("contacts").unwrap();
        assert_eq!(contacts.path, "/contacts");
        assert_eq!(contacts.columns.len(), 3);
        assert!(matches!(&contacts.row_path, RowPath::Pointer { pointer } if pointer == "/data"));
        assert!(matches!(contacts.pagination, Pagination::None));
        assert!(contacts.filters.is_empty());

        let companies = spec.table("companies").unwrap();
        assert_eq!(companies.path, "/companies");
        assert_eq!(companies.columns.len(), 3);
        assert!(matches!(&companies.row_path, RowPath::Pointer { pointer } if pointer == "/data"));

        let admins = spec.table("admins").unwrap();
        assert_eq!(admins.path, "/admins");
        assert_eq!(admins.columns.len(), 3);
        assert!(matches!(&admins.row_path, RowPath::Pointer { pointer } if pointer == "/admins"));

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_contacts_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/contacts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": "1", "email": "a@example.com", "name": "Ada"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = IntercomConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(intercom_spec(&cfg))
            .fetch("contacts", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
