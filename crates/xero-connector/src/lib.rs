//! # xero-connector
//!
//! The Xero connector for BudBuk. Like the other REST connectors, it is mostly
//! *config*: it builds a [`rest_connector::SourceSpec`] describing a few Xero
//! Accounting API collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! Xero needs more than one static header, so it authenticates with
//! [`AuthSpec::Headers`]: a bearer `Authorization`, the `Xero-tenant-id` naming
//! the organisation, and `Accept: application/json`.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Xero target. `base_url` points at the Accounting API (or a test server);
/// `token` is the OAuth2 access token and `tenant_id` names the organisation.
#[derive(Debug, Clone)]
pub struct XeroConfig {
    pub base_url: String,
    pub token: String,
    pub tenant_id: String,
}

/// Build the Xero source spec for a config. This is the *entire* connector:
/// endpoints, columns, and multi-header auth, as data.
pub fn xero_spec(cfg: &XeroConfig) -> SourceSpec {
    let col = |name: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: name.to_string(),
        data_type,
    };
    // Xero returns records under a top-level array keyed by the collection name.
    let table = |name: &str, columns: Vec<ColumnSpec>| TableSpec {
        name: name.to_string(),
        path: format!("/{name}"),
        row_path: RowPath::Pointer {
            pointer: format!("/{name}"),
        },
        columns,
        pagination: Pagination::None,
        filters: vec![],
    };

    SourceSpec {
        name: "xero".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Headers {
            headers: vec![
                ("Authorization".to_string(), format!("Bearer {}", cfg.token)),
                ("Xero-tenant-id".to_string(), cfg.tenant_id.clone()),
                ("Accept".to_string(), "application/json".to_string()),
            ],
        },
        tables: vec![
            table(
                "Invoices",
                vec![
                    col("InvoiceID", DataType::Text),
                    col("Status", DataType::Text),
                    col("Total", DataType::Float),
                ],
            ),
            table(
                "Contacts",
                vec![
                    col("ContactID", DataType::Text),
                    col("Name", DataType::Text),
                    col("EmailAddress", DataType::Text),
                ],
            ),
            table(
                "Accounts",
                vec![
                    col("AccountID", DataType::Text),
                    col("Name", DataType::Text),
                    col("Type", DataType::Text),
                ],
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use connector_sdk::{Connector, Query};
    use rest_connector::RestConnector;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn spec_exposes_expected_tables_and_header_auth() {
        let cfg = XeroConfig {
            base_url: "https://api.xero.com/api.xro/2.0".to_string(),
            token: "at_x".to_string(),
            tenant_id: "tenant_x".to_string(),
        };
        let spec = xero_spec(&cfg);
        assert_eq!(spec.name, "xero");
        assert_eq!(spec.base_url, "https://api.xero.com/api.xro/2.0");
        assert!(matches!(&spec.auth, AuthSpec::Headers { .. }));
        // Exact Debug comparison verifies the header set, values, and order
        // without an `if let`/`match` fall-through arm (which llvm-cov would
        // count as an uncovered line).
        assert_eq!(
            format!("{:?}", spec.auth),
            r#"Headers { headers: [("Authorization", "Bearer at_x"), ("Xero-tenant-id", "tenant_x"), ("Accept", "application/json")] }"#
        );
        for (name, cols) in [
            ("Invoices", ["InvoiceID", "Status", "Total"]),
            ("Contacts", ["ContactID", "Name", "EmailAddress"]),
            ("Accounts", ["AccountID", "Name", "Type"]),
        ] {
            let t = spec.table(name).expect("table present");
            assert_eq!(t.path, format!("/{name}"));
            assert!(
                matches!(&t.row_path, RowPath::Pointer { pointer } if pointer == &format!("/{name}"))
            );
            assert!(matches!(t.pagination, Pagination::None));
            assert!(t.filters.is_empty());
            assert_eq!(t.columns.len(), 3);
            for (i, cname) in cols.iter().enumerate() {
                assert_eq!(&t.columns[i].name, cname);
                assert_eq!(&t.columns[i].field, cname);
            }
        }
        assert!(spec.table("missing").is_none());
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_invoices_from_mock_sends_tenant_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/Invoices"))
            .and(header("Xero-tenant-id", "tenant_x"))
            .and(header("Authorization", "Bearer at_x"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Invoices": [
                    {"InvoiceID": "inv-1", "Status": "PAID", "Total": 42.5}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = XeroConfig {
            base_url: server.uri(),
            token: "at_x".to_string(),
            tenant_id: "tenant_x".to_string(),
        };
        let rows = RestConnector::new(xero_spec(&cfg))
            .fetch("Invoices", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
