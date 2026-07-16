//! # shopify-connector
//!
//! The Shopify connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Shopify Admin REST collections and runs on the shared `RestConnector`
//! engine (caching, tracing, pushdown, FDW — all for free).
//!
//! Shopify authenticates with a private-app access token sent in the
//! `X-Shopify-Access-Token` header.

use connector_sdk::DataType;
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec,
};

/// One Shopify target. `base_url` is the shop's Admin API base (e.g.
/// `https://acme.myshopify.com/admin/api/2024-01`); `access_token` is the
/// private-app token sent in `X-Shopify-Access-Token`.
#[derive(Debug, Clone)]
pub struct ShopifyConfig {
    pub base_url: String,
    pub access_token: String,
}

/// Build the Shopify source spec for a config. This is the *entire* connector:
/// endpoints, columns, pagination, and pushdown, as data.
pub fn shopify_spec(cfg: &ShopifyConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "shopify".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::ApiKeyHeader {
            header: "X-Shopify-Access-Token".to_string(),
            value: cfg.access_token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "products".to_string(),
                path: "/products.json".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/products".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("title", "title", DataType::Text),
                    col("status", "status", DataType::Text),
                    col("vendor", "vendor", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![FilterParam {
                    column: "status".to_string(),
                    param: "status".to_string(),
                }],
            },
            TableSpec {
                name: "orders".to_string(),
                path: "/orders.json".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/orders".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("name", "name", DataType::Text),
                    col("total_price", "total_price", DataType::Text),
                    col("financial_status", "financial_status", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "customers".to_string(),
                path: "/customers.json".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/customers".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("email", "email", DataType::Text),
                    col("first_name", "first_name", DataType::Text),
                    col("last_name", "last_name", DataType::Text),
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
    use connector_sdk::{Connector, Filter, Operator, Query, Value};
    use rest_connector::RestConnector;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn spec_exposes_expected_tables_and_header_auth() {
        let cfg = ShopifyConfig {
            base_url: "https://acme.myshopify.com/admin/api/2024-01".to_string(),
            access_token: "shpat_x".to_string(),
        };
        let spec = shopify_spec(&cfg);
        assert_eq!(spec.name, "shopify");
        assert_eq!(spec.base_url, cfg.base_url);
        assert!(matches!(
            spec.auth,
            AuthSpec::ApiKeyHeader { ref header, .. } if header == "X-Shopify-Access-Token"
        ));
        assert!(spec.table("products").is_some());
        assert!(spec.table("orders").is_some());
        assert!(spec.table("customers").is_some());
        assert_eq!(spec.table("products").unwrap().columns.len(), 4);
        assert_eq!(spec.table("orders").unwrap().columns.len(), 4);
        assert_eq!(spec.table("customers").unwrap().columns.len(), 4);
        assert!(matches!(
            spec.table("products").unwrap().pagination,
            Pagination::None
        ));
        assert!(matches!(
            spec.table("products").unwrap().row_path,
            RowPath::Pointer { ref pointer } if pointer == "/products"
        ));
        assert_eq!(spec.table("products").unwrap().filters[0].param, "status");
        assert!(spec.table("orders").unwrap().filters.is_empty());
        assert!(spec.table("customers").unwrap().filters.is_empty());
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_products_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/products.json"))
            .and(header("X-Shopify-Access-Token", "shpat_x"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "products": [
                    {"id":1,"title":"Widget","status":"active","vendor":"Acme"},
                    {"id":2,"title":"Gadget","status":"draft","vendor":"Acme"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = ShopifyConfig {
            base_url: server.uri(),
            access_token: "shpat_x".into(),
        };
        let rows = RestConnector::new(shopify_spec(&cfg))
            .fetch("products", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn status_filter_is_pushed_down() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/products.json"))
            .and(query_param("status", "active"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "products": [
                    {"id":1,"title":"Widget","status":"active","vendor":"Acme"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = ShopifyConfig {
            base_url: server.uri(),
            access_token: "shpat_x".into(),
        };
        let query = Query {
            filters: vec![Filter {
                column: "status".to_string(),
                op: Operator::Eq,
                value: Value::Text("active".to_string()),
            }],
            ..Query::default()
        };
        let rows = RestConnector::new(shopify_spec(&cfg))
            .fetch("products", &query)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
