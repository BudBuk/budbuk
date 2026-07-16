//! BudBuk connector for Freshdesk.
//!
//! Exposes Freshdesk's `tickets`, `contacts`, and `companies` endpoints as tables
//! driven entirely by a declarative [`SourceSpec`]. The [`RestConnector`] engine
//! performs all HTTP and page-based pagination.
//!
//! Freshdesk uses the API key as the basic-auth username with a dummy password,
//! and returns bare JSON arrays (hence [`RowPath::Root`]).
//!
//! [`RestConnector`]: rest_connector::RestConnector

use connector_sdk::DataType;
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec,
};

/// Configuration for connecting to a Freshdesk instance.
#[derive(Debug, Clone)]
pub struct FreshdeskConfig {
    /// Instance base URL, e.g. `"https://acme.freshdesk.com"`.
    pub base_url: String,
    /// The Freshdesk API key (used as the basic-auth username).
    pub api_key: String,
}

/// Standard Freshdesk page-based pagination: `?page=1&per_page=30`.
fn freshdesk_pagination() -> Pagination {
    Pagination::Page {
        page_param: "page".to_string(),
        size_param: "per_page".to_string(),
        page_size: 30,
        start_page: 1,
    }
}

/// Build a column spec from its name, type, and dotted JSON field path.
fn col(name: &str, data_type: DataType, field: &str) -> ColumnSpec {
    ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    }
}

/// Build the declarative [`SourceSpec`] describing the Freshdesk source.
pub fn freshdesk_spec(cfg: &FreshdeskConfig) -> SourceSpec {
    SourceSpec {
        name: "freshdesk".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Basic {
            username: cfg.api_key.clone(),
            password: "X".to_string(),
        },
        tables: vec![
            TableSpec {
                name: "tickets".to_string(),
                path: "/api/v2/tickets".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", DataType::Integer, "id"),
                    col("subject", DataType::Text, "subject"),
                    col("status", DataType::Integer, "status"),
                    col("priority", DataType::Integer, "priority"),
                    col("requester_id", DataType::Integer, "requester_id"),
                    col("created_at", DataType::Timestamp, "created_at"),
                ],
                pagination: freshdesk_pagination(),
                filters: vec![] as Vec<FilterParam>,
            },
            TableSpec {
                name: "contacts".to_string(),
                path: "/api/v2/contacts".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", DataType::Integer, "id"),
                    col("name", DataType::Text, "name"),
                    col("email", DataType::Text, "email"),
                    col("phone", DataType::Text, "phone"),
                    col("created_at", DataType::Timestamp, "created_at"),
                ],
                pagination: freshdesk_pagination(),
                filters: vec![],
            },
            TableSpec {
                name: "companies".to_string(),
                path: "/api/v2/companies".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", DataType::Integer, "id"),
                    col("name", DataType::Text, "name"),
                    col("created_at", DataType::Timestamp, "created_at"),
                ],
                pagination: freshdesk_pagination(),
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

    fn test_cfg() -> FreshdeskConfig {
        FreshdeskConfig {
            base_url: "https://acme.freshdesk.com".to_string(),
            api_key: "secret-key".to_string(),
        }
    }

    #[test]
    fn spec_shape() {
        let cfg = test_cfg();
        let spec = freshdesk_spec(&cfg);
        assert_eq!(spec.name, "freshdesk");
        assert_eq!(spec.base_url, "https://acme.freshdesk.com");
        assert_eq!(spec.tables.len(), 3);

        let tickets = spec.table("tickets").expect("tickets table");
        assert_eq!(tickets.path, "/api/v2/tickets");
        assert_eq!(tickets.columns.len(), 6);
        assert!(matches!(tickets.row_path, RowPath::Root));

        let contacts = spec.table("contacts").expect("contacts table");
        assert_eq!(contacts.path, "/api/v2/contacts");
        assert_eq!(contacts.columns.len(), 5);
        assert!(matches!(contacts.row_path, RowPath::Root));

        let companies = spec.table("companies").expect("companies table");
        assert_eq!(companies.path, "/api/v2/companies");
        assert_eq!(companies.columns.len(), 3);
        assert!(matches!(companies.row_path, RowPath::Root));

        // The API key is used as the basic-auth username with a dummy password.
        assert_eq!(
            serde_json::to_value(&spec.auth).unwrap(),
            serde_json::json!({
                "type": "basic",
                "username": cfg.api_key,
                "password": "X",
            })
        );

        for table in &spec.tables {
            assert_eq!(
                serde_json::to_value(&table.pagination).unwrap(),
                serde_json::json!({
                    "style": "page",
                    "page_param": "page",
                    "size_param": "per_page",
                    "page_size": 30,
                    "start_page": 1,
                })
            );
            assert!(table.filters.is_empty());
        }
    }

    #[tokio::test]
    async fn fetch_tickets_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/tickets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id":1,"subject":"Help","status":2,"priority":1}
            ])))
            .mount(&server)
            .await;
        let cfg = FreshdeskConfig {
            base_url: server.uri(),
            api_key: "key".into(),
        };
        let rows = RestConnector::new(freshdesk_spec(&cfg))
            .fetch("tickets", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
