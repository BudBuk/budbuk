//! # zohocrm-connector
//!
//! The Zoho CRM connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Zoho CRM record modules (Leads, Contacts, Accounts, Deals) and runs on the
//! shared `RestConnector` engine (caching, tracing, pushdown, FDW — for free).
//!
//! Zoho CRM authenticates with an OAuth token sent as
//! `Authorization: Zoho-oauthtoken <token>` and wraps record arrays under a
//! `"data"` key. It paginates with `?page=N&per_page=M` (1-based).

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Zoho CRM target. `base_url` supports the regional API host and tests;
/// `token` is the OAuth token sent in the `Authorization` header.
#[derive(Debug, Clone)]
pub struct ZohoCrmConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Zoho CRM source spec for a config. This is the *entire* connector:
/// endpoints, columns, pagination, and auth, as data.
pub fn zohocrm_spec(cfg: &ZohoCrmConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Zoho CRM paginates with ?page=N&per_page=M (1-based), max 200 per page.
    let page = || Pagination::Page {
        page_param: "page".to_string(),
        size_param: "per_page".to_string(),
        page_size: 200,
        start_page: 1,
    };
    // Record modules all wrap their array under the "data" key.
    let data = || RowPath::Pointer {
        pointer: "/data".to_string(),
    };

    SourceSpec {
        name: "zohocrm".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::ApiKeyHeader {
            header: "Authorization".to_string(),
            value: format!("Zoho-oauthtoken {}", cfg.token),
        },
        tables: vec![
            TableSpec {
                name: "Leads".to_string(),
                path: "/Leads".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("Email", "Email", DataType::Text),
                    col("Company", "Company", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "Contacts".to_string(),
                path: "/Contacts".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("Email", "Email", DataType::Text),
                    col("Full_Name", "Full_Name", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "Accounts".to_string(),
                path: "/Accounts".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("Account_Name", "Account_Name", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "Deals".to_string(),
                path: "/Deals".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("Deal_Name", "Deal_Name", DataType::Text),
                    col("Stage", "Stage", DataType::Text),
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
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(base_url: String) -> ZohoCrmConfig {
        ZohoCrmConfig {
            base_url,
            token: "tok123".to_string(),
        }
    }

    #[test]
    fn spec_exposes_expected_tables_and_oauth_header() {
        let cfg = cfg("https://www.zohoapis.com/crm/v2".to_string());
        let spec = zohocrm_spec(&cfg);
        assert_eq!(spec.name, "zohocrm");
        assert_eq!(spec.base_url, "https://www.zohoapis.com/crm/v2");
        // Auth is a fixed Authorization header formatted as Zoho-oauthtoken <token>.
        assert!(matches!(
            &spec.auth,
            AuthSpec::ApiKeyHeader { header, value }
                if header == "Authorization" && value == "Zoho-oauthtoken tok123"
        ));

        // All four modules are present with the right columns.
        assert_eq!(spec.table("Leads").unwrap().columns.len(), 3);
        assert_eq!(spec.table("Contacts").unwrap().columns.len(), 3);
        assert_eq!(spec.table("Accounts").unwrap().columns.len(), 2);
        assert_eq!(spec.table("Deals").unwrap().columns.len(), 3);
        assert!(spec.table("Missing").is_none());

        // Every module wraps rows under /data, has empty filters, and pages.
        for name in ["Leads", "Contacts", "Accounts", "Deals"] {
            let t = spec.table(name).unwrap();
            assert_eq!(t.path, format!("/{name}"));
            assert!(matches!(&t.row_path, RowPath::Pointer { pointer } if pointer == "/data"));
            assert!(t.filters.is_empty());
            assert!(matches!(
                &t.pagination,
                Pagination::Page { page_param, size_param, page_size, start_page }
                    if page_param == "page"
                        && size_param == "per_page"
                        && *page_size == 200
                        && *start_page == 1
            ));
        }

        // A couple of specific field mappings.
        assert_eq!(spec.table("Deals").unwrap().columns[1].field, "Deal_Name");
        assert_eq!(spec.table("Contacts").unwrap().columns[2].name, "Full_Name");

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_leads_from_mock_sends_oauth_header() {
        let server = MockServer::start().await;
        // Fewer than page_size (200) rows so Page pagination terminates.
        Mock::given(method("GET"))
            .and(path("/Leads"))
            .and(header("Authorization", "Zoho-oauthtoken tok123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": "1", "Email": "a@x.com", "Company": "Acme"},
                    {"id": "2", "Email": "b@x.com", "Company": "Globex"}
                ]
            })))
            .mount(&server)
            .await;

        let rows = RestConnector::new(zohocrm_spec(&cfg(server.uri())))
            .fetch("Leads", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn fetch_deals_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/Deals"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": "9", "Deal_Name": "Big", "Stage": "Won"}
                ]
            })))
            .mount(&server)
            .await;

        let rows = RestConnector::new(zohocrm_spec(&cfg(server.uri())))
            .fetch("Deals", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
