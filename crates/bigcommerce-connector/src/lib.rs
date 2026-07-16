//! # bigcommerce-connector
//!
//! The BigCommerce connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! BigCommerce Catalog API collections and runs on the shared `RestConnector`
//! engine (caching, tracing, pushdown, FDW — all for free).
//!
//! BigCommerce authenticates with a store-scoped access token sent in the
//! `X-Auth-Token` header, and wraps list responses in a `{"data": [...]}`
//! envelope paginated with `?page=N&limit=M` (1-based).

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One BigCommerce store target. `base_url` points at the store's API root
/// (e.g. `https://api.bigcommerce.com/stores/<hash>/v3`); `access_token`
/// authenticates via the `X-Auth-Token` header.
#[derive(Debug, Clone)]
pub struct BigCommerceConfig {
    pub base_url: String,
    pub access_token: String,
}

/// Build the BigCommerce source spec for a config. This is the *entire*
/// connector: endpoints, columns, and pagination, as data.
pub fn bigcommerce_spec(cfg: &BigCommerceConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // BigCommerce paginates with ?page=N&limit=M (1-based) inside a
    // {"data": [...]} envelope.
    let page = || Pagination::Page {
        page_param: "page".to_string(),
        size_param: "limit".to_string(),
        page_size: 250,
        start_page: 1,
    };
    let data = || RowPath::Pointer {
        pointer: "/data".to_string(),
    };

    SourceSpec {
        name: "bigcommerce".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::ApiKeyHeader {
            header: "X-Auth-Token".to_string(),
            value: cfg.access_token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "products".to_string(),
                path: "/catalog/products".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("name", "name", DataType::Text),
                    col("sku", "sku", DataType::Text),
                    col("price", "price", DataType::Float),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "categories".to_string(),
                path: "/catalog/categories".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("name", "name", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "brands".to_string(),
                path: "/catalog/brands".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Integer),
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
    fn spec_exposes_expected_tables_and_header_auth() {
        let cfg = BigCommerceConfig {
            base_url: "https://api.bigcommerce.com/stores/abc/v3".to_string(),
            access_token: "tok".to_string(),
        };
        let spec = bigcommerce_spec(&cfg);
        assert_eq!(spec.name, "bigcommerce");
        assert_eq!(spec.base_url, "https://api.bigcommerce.com/stores/abc/v3");
        assert!(matches!(
            spec.auth,
            AuthSpec::ApiKeyHeader { ref header, ref value }
                if header == "X-Auth-Token" && value == "tok"
        ));
        assert!(spec.table("products").is_some());
        assert!(spec.table("categories").is_some());
        assert!(spec.table("brands").is_some());
        assert_eq!(spec.table("products").unwrap().columns.len(), 4);
        assert_eq!(spec.table("categories").unwrap().columns.len(), 2);
        assert_eq!(spec.table("brands").unwrap().columns.len(), 2);
        // All tables read from the /data envelope with Page pagination.
        for name in ["products", "categories", "brands"] {
            let t = spec.table(name).unwrap();
            assert!(t.filters.is_empty());
            assert!(matches!(
                t.row_path,
                RowPath::Pointer { ref pointer } if pointer == "/data"
            ));
            assert!(matches!(
                t.pagination,
                Pagination::Page {
                    ref page_param,
                    ref size_param,
                    page_size: 250,
                    start_page: 1,
                } if page_param == "page" && size_param == "limit"
            ));
        }
        assert_eq!(spec.table("products").unwrap().path, "/catalog/products");
        assert_eq!(
            spec.table("categories").unwrap().path,
            "/catalog/categories"
        );
        assert_eq!(spec.table("brands").unwrap().path, "/catalog/brands");
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_products_from_mock() {
        let server = MockServer::start().await;
        // Return fewer rows than page_size (250) so Page pagination terminates.
        Mock::given(method("GET"))
            .and(path("/catalog/products"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": 1, "name": "Widget", "sku": "W-1", "price": 9.99},
                    {"id": 2, "name": "Gadget", "sku": "G-2", "price": 19.5}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = BigCommerceConfig {
            base_url: server.uri(),
            access_token: "t".into(),
        };
        let rows = RestConnector::new(bigcommerce_spec(&cfg))
            .fetch("products", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
