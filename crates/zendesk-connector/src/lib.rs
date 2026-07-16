//! BudBuk connector for Zendesk.
//!
//! Exposes Zendesk's `tickets`, `users`, and `organizations` endpoints as tables
//! driven entirely by a declarative [`SourceSpec`]. The [`RestConnector`] engine
//! performs all HTTP, page-based pagination, and predicate pushdown.
//!
//! [`RestConnector`]: rest_connector::RestConnector

use connector_sdk::DataType;
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec,
};

/// Configuration for connecting to a Zendesk instance.
#[derive(Debug, Clone)]
pub struct ZendeskConfig {
    /// Instance base URL, e.g. `"https://acme.zendesk.com"`.
    pub base_url: String,
    /// The account email used for email/token basic auth.
    pub email: String,
    /// The Zendesk API token.
    pub api_token: String,
}

/// Standard Zendesk page-based pagination: `?page=1&per_page=100`.
fn zendesk_pagination() -> Pagination {
    Pagination::Page {
        page_param: "page".to_string(),
        size_param: "per_page".to_string(),
        page_size: 100,
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

/// Build the declarative [`SourceSpec`] describing the Zendesk source.
pub fn zendesk_spec(cfg: &ZendeskConfig) -> SourceSpec {
    SourceSpec {
        name: "zendesk".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Basic {
            username: format!("{}/token", cfg.email),
            password: cfg.api_token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "tickets".to_string(),
                path: "/api/v2/tickets.json".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/tickets".to_string(),
                },
                columns: vec![
                    col("id", DataType::Integer, "id"),
                    col("subject", DataType::Text, "subject"),
                    col("status", DataType::Text, "status"),
                    col("priority", DataType::Text, "priority"),
                    col("requester_id", DataType::Integer, "requester_id"),
                    col("assignee_id", DataType::Integer, "assignee_id"),
                    col("created_at", DataType::Timestamp, "created_at"),
                ],
                pagination: zendesk_pagination(),
                filters: vec![] as Vec<FilterParam>,
            },
            TableSpec {
                name: "users".to_string(),
                path: "/api/v2/users.json".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/users".to_string(),
                },
                columns: vec![
                    col("id", DataType::Integer, "id"),
                    col("name", DataType::Text, "name"),
                    col("email", DataType::Text, "email"),
                    col("role", DataType::Text, "role"),
                    col("created_at", DataType::Timestamp, "created_at"),
                ],
                pagination: zendesk_pagination(),
                filters: vec![],
            },
            TableSpec {
                name: "organizations".to_string(),
                path: "/api/v2/organizations.json".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/organizations".to_string(),
                },
                columns: vec![
                    col("id", DataType::Integer, "id"),
                    col("name", DataType::Text, "name"),
                    col("created_at", DataType::Timestamp, "created_at"),
                ],
                pagination: zendesk_pagination(),
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

    fn test_cfg() -> ZendeskConfig {
        ZendeskConfig {
            base_url: "https://acme.zendesk.com".to_string(),
            email: "a@b.c".to_string(),
            api_token: "secret".to_string(),
        }
    }

    #[test]
    fn spec_shape() {
        let spec = zendesk_spec(&test_cfg());
        assert_eq!(spec.name, "zendesk");
        assert_eq!(spec.base_url, "https://acme.zendesk.com");

        let tickets = spec.table("tickets").expect("tickets table");
        assert_eq!(tickets.path, "/api/v2/tickets.json");
        assert_eq!(tickets.columns.len(), 7);
        assert!(matches!(&tickets.row_path, RowPath::Pointer { pointer } if pointer == "/tickets"));

        let users = spec.table("users").expect("users table");
        assert_eq!(users.path, "/api/v2/users.json");
        assert_eq!(users.columns.len(), 5);
        assert!(matches!(&users.row_path, RowPath::Pointer { pointer } if pointer == "/users"));

        let orgs = spec.table("organizations").expect("organizations table");
        assert_eq!(orgs.path, "/api/v2/organizations.json");
        assert_eq!(orgs.columns.len(), 3);
        assert!(
            matches!(&orgs.row_path, RowPath::Pointer { pointer } if pointer == "/organizations")
        );
    }

    #[test]
    fn pagination_and_auth() {
        let spec = zendesk_spec(&test_cfg());
        assert!(matches!(
            &spec.auth,
            AuthSpec::Basic { username, password }
                if username == "a@b.c/token" && username.ends_with("/token") && password == "secret"
        ));
        for table in &spec.tables {
            assert!(matches!(
                &table.pagination,
                Pagination::Page { page_param, size_param, page_size, start_page }
                    if page_param == "page"
                        && size_param == "per_page"
                        && *page_size == 100
                        && *start_page == 1
            ));
            assert!(table.filters.is_empty());
        }
    }

    #[tokio::test]
    async fn fetch_tickets_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/tickets.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tickets": [{"id":1,"subject":"Help","status":"open"}]
            })))
            .mount(&server)
            .await;
        let cfg = ZendeskConfig {
            base_url: server.uri(),
            email: "a@b.c".into(),
            api_token: "t".into(),
        };
        let rows = RestConnector::new(zendesk_spec(&cfg))
            .fetch("tickets", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
