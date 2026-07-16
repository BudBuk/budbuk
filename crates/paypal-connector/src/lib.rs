//! # paypal-connector
//!
//! The PayPal connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! PayPal REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! It authenticates with a bearer access token.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One PayPal target. `base_url` supports sandbox/live hosts and tests; `token`
/// is the OAuth2 access token sent as a bearer token.
#[derive(Debug, Clone)]
pub struct PaypalConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the PayPal source spec for a config. This is the *entire* connector:
/// endpoints, columns, and row pointers, as data.
pub fn paypal_spec(cfg: &PaypalConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "paypal".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "invoices".to_string(),
                path: "/v2/invoicing/invoices".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/items".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("status", "status", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "plans".to_string(),
                path: "/v1/billing/plans".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/plans".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("status", "status", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "products".to_string(),
                path: "/v1/catalogs/products".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/products".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
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
        let cfg = PaypalConfig {
            base_url: "https://api-m.paypal.com".to_string(),
            token: "A21AA".to_string(),
        };
        let spec = paypal_spec(&cfg);
        assert_eq!(spec.name, "paypal");
        assert_eq!(spec.base_url, "https://api-m.paypal.com");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));

        let invoices = spec.table("invoices").unwrap();
        assert_eq!(invoices.path, "/v2/invoicing/invoices");
        assert!(matches!(
            &invoices.row_path,
            RowPath::Pointer { pointer } if pointer == "/items"
        ));
        assert_eq!(invoices.columns.len(), 2);
        assert!(matches!(invoices.pagination, Pagination::None));
        assert!(invoices.filters.is_empty());

        let plans = spec.table("plans").unwrap();
        assert_eq!(plans.path, "/v1/billing/plans");
        assert!(matches!(
            &plans.row_path,
            RowPath::Pointer { pointer } if pointer == "/plans"
        ));
        assert_eq!(plans.columns.len(), 3);

        let products = spec.table("products").unwrap();
        assert_eq!(products.path, "/v1/catalogs/products");
        assert!(matches!(
            &products.row_path,
            RowPath::Pointer { pointer } if pointer == "/products"
        ));
        assert_eq!(products.columns.len(), 2);

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_invoices_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/invoicing/invoices"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {"id": "INV2-1", "status": "SENT"},
                    {"id": "INV2-2", "status": "PAID"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = PaypalConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(paypal_spec(&cfg))
            .fetch("invoices", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
