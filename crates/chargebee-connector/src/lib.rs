//! BudBuk connector for Chargebee.
//!
//! Exposes Chargebee's REST API as tables (`subscriptions`, `customers`,
//! `invoices`) via a declarative [`SourceSpec`]. Chargebee returns each list
//! endpoint as `{"list": [ {"subscription": {...}}, ... ]}`: the records live
//! under `/list`, and each item wraps the resource in a typed key. Rows are
//! therefore extracted with [`RowPath::Pointer`] at `/list`, and each column's
//! `field` reaches *into* the wrapper with a dotted path (e.g. `subscription.id`).
//!
//! Chargebee authenticates with HTTP Basic auth, sending the API key as the
//! username and an empty password.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// Configuration for a Chargebee source.
///
/// `base_url` looks like `"https://SITE.chargebee.com/api/v2"`; `api_key` is the
/// site API key, sent as the HTTP Basic username with an empty password.
#[derive(Debug, Clone)]
pub struct ChargebeeConfig {
    pub base_url: String,
    pub api_key: String,
}

/// Build the [`SourceSpec`] describing the Chargebee REST API.
pub fn chargebee_spec(cfg: &ChargebeeConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Every Chargebee list endpoint wraps records in `{"list": [...]}`.
    let list = || RowPath::Pointer {
        pointer: "/list".to_string(),
    };

    let subscriptions = TableSpec {
        name: "subscriptions".to_string(),
        path: "/subscriptions".to_string(),
        row_path: list(),
        columns: vec![
            col("id", "subscription.id", DataType::Text),
            col("status", "subscription.status", DataType::Text),
        ],
        pagination: Pagination::None,
        filters: vec![],
    };

    let customers = TableSpec {
        name: "customers".to_string(),
        path: "/customers".to_string(),
        row_path: list(),
        columns: vec![
            col("id", "customer.id", DataType::Text),
            col("email", "customer.email", DataType::Text),
        ],
        pagination: Pagination::None,
        filters: vec![],
    };

    let invoices = TableSpec {
        name: "invoices".to_string(),
        path: "/invoices".to_string(),
        row_path: list(),
        columns: vec![
            col("id", "invoice.id", DataType::Text),
            col("status", "invoice.status", DataType::Text),
        ],
        pagination: Pagination::None,
        filters: vec![],
    };

    SourceSpec {
        name: "chargebee".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Basic {
            username: cfg.api_key.clone(),
            password: String::new(),
        },
        tables: vec![subscriptions, customers, invoices],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use connector_sdk::{Connector, Query};
    use rest_connector::RestConnector;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn spec() -> SourceSpec {
        chargebee_spec(&ChargebeeConfig {
            base_url: "https://acme.chargebee.com/api/v2".to_string(),
            api_key: "live_key".to_string(),
        })
    }

    #[test]
    fn spec_has_expected_shape() {
        let cfg = ChargebeeConfig {
            base_url: "https://acme.chargebee.com/api/v2".to_string(),
            api_key: "live_key".to_string(),
        };
        let s = chargebee_spec(&cfg);
        assert_eq!(s.name, "chargebee");
        assert_eq!(s.base_url, "https://acme.chargebee.com/api/v2");

        // Basic auth: API key as username, empty password.
        assert!(matches!(
            &s.auth,
            AuthSpec::Basic { username, password }
                if username == "live_key" && password.is_empty()
        ));

        // Three tables with the expected paths, all using Pointer "/list",
        // Pagination::None, and no filters.
        assert_eq!(s.tables.len(), 3);
        let names: Vec<&str> = s.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["subscriptions", "customers", "invoices"]);

        for (t, expected_path) in s
            .tables
            .iter()
            .zip(["/subscriptions", "/customers", "/invoices"])
        {
            assert_eq!(t.path, expected_path);
            assert!(matches!(&t.row_path, RowPath::Pointer { pointer } if pointer == "/list"));
            assert!(matches!(t.pagination, Pagination::None));
            assert!(t.filters.is_empty());
            assert_eq!(t.columns.len(), 2);
        }

        // Columns reach into the wrapper object with dotted paths.
        let subs = spec();
        let subs = subs.table("subscriptions").unwrap();
        assert_eq!(subs.columns[0].field, "subscription.id");
        assert_eq!(subs.columns[1].field, "subscription.status");
        let customers = s.table("customers").unwrap();
        assert_eq!(customers.columns[0].field, "customer.id");
        assert_eq!(customers.columns[1].field, "customer.email");
        let invoices = s.table("invoices").unwrap();
        assert_eq!(invoices.columns[0].field, "invoice.id");
        assert_eq!(invoices.columns[1].field, "invoice.status");

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_subscriptions_reads_nested_wrapper_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/subscriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "list": [{"subscription": {"id": "sub_1", "status": "active"}}]
            })))
            .mount(&server)
            .await;
        let cfg = ChargebeeConfig {
            base_url: server.uri(),
            api_key: "k".into(),
        };
        let rows = RestConnector::new(chargebee_spec(&cfg))
            .fetch("subscriptions", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        // First column `id` reads the nested "subscription.id" -> "sub_1".
        assert_eq!(rows[0].0[0].to_display_string(), "sub_1");
        assert_eq!(rows[0].0[1].to_display_string(), "active");
    }
}
