//! # square-connector
//!
//! The Square connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Square API collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! Square authenticates with a personal/OAuth access token sent as a bearer
//! token.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Square target. `base_url` selects the environment (production or
/// sandbox) and supports tests; `token` authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct SquareConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Square source spec for a config. This is the *entire* connector:
/// endpoints, columns, and row locations, as data.
pub fn square_spec(cfg: &SquareConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "square".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "customers".to_string(),
                path: "/customers".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/customers".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("given_name", "given_name", DataType::Text),
                    col("family_name", "family_name", DataType::Text),
                    col("email_address", "email_address", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "payments".to_string(),
                path: "/payments".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/payments".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("status", "status", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "locations".to_string(),
                path: "/locations".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/locations".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("status", "status", DataType::Text),
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
        let cfg = SquareConfig {
            base_url: "https://connect.squareup.com/v2".to_string(),
            token: "sq0atp_x".to_string(),
        };
        let spec = square_spec(&cfg);
        assert_eq!(spec.name, "square");
        assert_eq!(spec.base_url, "https://connect.squareup.com/v2");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("customers").is_some());
        assert!(spec.table("payments").is_some());
        assert!(spec.table("locations").is_some());
        assert_eq!(spec.table("customers").unwrap().columns.len(), 4);
        assert_eq!(spec.table("payments").unwrap().columns.len(), 2);
        assert_eq!(spec.table("locations").unwrap().columns.len(), 3);
        // Every table is unpaginated with no pushdown filters.
        for name in ["customers", "payments", "locations"] {
            let t = spec.table(name).unwrap();
            assert!(matches!(t.pagination, Pagination::None));
            assert!(t.filters.is_empty());
            assert!(matches!(t.row_path, RowPath::Pointer { .. }));
        }
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_customers_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/customers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "customers": [
                    {"id":"C1","given_name":"Ada","family_name":"Lovelace","email_address":"ada@x.io"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = SquareConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(square_spec(&cfg))
            .fetch("customers", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
