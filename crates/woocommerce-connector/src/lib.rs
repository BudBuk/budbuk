//! # woocommerce-connector
//!
//! The WooCommerce connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! WooCommerce REST (`/wp-json/wc/v3`) collections and runs on the shared
//! `RestConnector` engine (caching, tracing, pushdown, FDW — all for free).
//!
//! WooCommerce authenticates with a consumer key/secret pair sent as HTTP Basic
//! auth (over HTTPS), so the config is just a base URL and those two secrets.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One WooCommerce store. `base_url` points at the store (paths are appended);
/// `consumer_key`/`consumer_secret` are the REST API credentials.
#[derive(Debug, Clone)]
pub struct WooCommerceConfig {
    pub base_url: String,
    pub consumer_key: String,
    pub consumer_secret: String,
}

/// Build the WooCommerce source spec for a config. This is the *entire*
/// connector: endpoints, columns, pagination, and pushdown, as data.
pub fn woocommerce_spec(cfg: &WooCommerceConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // WooCommerce paginates with ?page=N&per_page=M (1-based).
    let page = || Pagination::Page {
        page_param: "page".to_string(),
        size_param: "per_page".to_string(),
        page_size: 100,
        start_page: 1,
    };

    SourceSpec {
        name: "woocommerce".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Basic {
            username: cfg.consumer_key.clone(),
            password: cfg.consumer_secret.clone(),
        },
        tables: vec![
            TableSpec {
                name: "products".to_string(),
                path: "/products".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("name", "name", DataType::Text),
                    col("status", "status", DataType::Text),
                    col("price", "price", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "orders".to_string(),
                path: "/orders".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("status", "status", DataType::Text),
                    col("total", "total", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "customers".to_string(),
                path: "/customers".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("email", "email", DataType::Text),
                    col("first_name", "first_name", DataType::Text),
                    col("last_name", "last_name", DataType::Text),
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
    fn spec_exposes_expected_tables_and_basic_auth() {
        let cfg = WooCommerceConfig {
            base_url: "https://store.example.com".to_string(),
            consumer_key: "ck_x".to_string(),
            consumer_secret: "cs_y".to_string(),
        };
        let spec = woocommerce_spec(&cfg);
        assert_eq!(spec.name, "woocommerce");
        assert_eq!(spec.base_url, "https://store.example.com");
        assert!(matches!(spec.auth, AuthSpec::Basic { .. }));
        assert!(spec.table("products").is_some());
        assert!(spec.table("orders").is_some());
        assert!(spec.table("customers").is_some());
        assert_eq!(spec.table("products").unwrap().columns.len(), 4);
        assert_eq!(spec.table("orders").unwrap().columns.len(), 3);
        assert_eq!(spec.table("customers").unwrap().columns.len(), 4);
        // All tables use Root row paths, Page pagination, and no filters.
        for name in ["products", "orders", "customers"] {
            let t = spec.table(name).unwrap();
            assert!(matches!(t.row_path, RowPath::Root));
            assert!(matches!(t.pagination, Pagination::Page { .. }));
            assert!(t.filters.is_empty());
        }
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_products_from_mock() {
        let server = MockServer::start().await;
        // Return fewer rows than page_size (100) so Page pagination terminates.
        Mock::given(method("GET"))
            .and(path("/products"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id":1,"name":"Cap","status":"publish","price":"9.99"},
                {"id":2,"name":"Mug","status":"publish","price":"12.50"}
            ])))
            .mount(&server)
            .await;
        let cfg = WooCommerceConfig {
            base_url: server.uri(),
            consumer_key: "ck".into(),
            consumer_secret: "cs".into(),
        };
        let rows = RestConnector::new(woocommerce_spec(&cfg))
            .fetch("products", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
