//! # notion-connector
//!
//! The Notion connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] and runs on the
//! shared `RestConnector` engine (caching, tracing, pushdown, FDW — all free).
//!
//! Notion authenticates with a bearer token *and* a required `Notion-Version`
//! header, so it uses [`AuthSpec::Headers`] to send both static headers.
//!
//! Notion's REST API is overwhelmingly POST/id-based (databases and pages are
//! queried by id via `POST /databases/{id}/query`, etc.), so there is no clean
//! collection to expose as a table. The one plain `GET` list endpoint is
//! `/users`, so — intentionally — this connector exposes a single `users` table.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Notion target. `base_url` is normally `https://api.notion.com` (kept
/// configurable for tests); `token` is the integration's bearer token.
#[derive(Debug, Clone)]
pub struct NotionConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Notion source spec for a config. This is the *entire* connector:
/// the endpoint, columns, and auth, expressed as data.
pub fn notion_spec(cfg: &NotionConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "notion".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Headers {
            headers: vec![
                ("Authorization".to_string(), format!("Bearer {}", cfg.token)),
                ("Notion-Version".to_string(), "2022-06-28".to_string()),
            ],
        },
        tables: vec![TableSpec {
            name: "users".to_string(),
            path: "/users".to_string(),
            row_path: RowPath::Pointer {
                pointer: "/results".to_string(),
            },
            columns: vec![
                col("id", "id", DataType::Text),
                col("name", "name", DataType::Text),
                col("type", "type", DataType::Text),
            ],
            pagination: Pagination::None,
            filters: vec![],
        }],
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
    fn spec_exposes_users_table_with_headers_auth() {
        let cfg = NotionConfig {
            base_url: "https://api.notion.com".to_string(),
            token: "secret_x".to_string(),
        };
        let spec = notion_spec(&cfg);
        assert_eq!(spec.name, "notion");
        assert_eq!(spec.base_url, "https://api.notion.com");
        // Auth carries both the bearer and the Notion-Version header.
        assert!(matches!(
            &spec.auth,
            AuthSpec::Headers { headers }
                if headers.len() == 2
                    && headers[0] == ("Authorization".to_string(), "Bearer secret_x".to_string())
                    && headers[1] == ("Notion-Version".to_string(), "2022-06-28".to_string())
        ));
        // Single table by design; users has three text columns via /results.
        assert_eq!(spec.tables.len(), 1);
        let users = spec.table("users").unwrap();
        assert_eq!(users.path, "/users");
        assert!(matches!(users.pagination, Pagination::None));
        assert!(users.filters.is_empty());
        assert!(matches!(&users.row_path, RowPath::Pointer { pointer } if pointer == "/results"));
        assert_eq!(users.columns.len(), 3);
        assert!(users
            .columns
            .iter()
            .all(|c| matches!(c.data_type, DataType::Text)));
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_users_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .and(header("Authorization", "Bearer secret_x"))
            .and(header("Notion-Version", "2022-06-28"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    {"id": "u1", "name": "Ada", "type": "person"},
                    {"id": "u2", "name": "Bot", "type": "bot"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = NotionConfig {
            base_url: server.uri(),
            token: "secret_x".to_string(),
        };
        let rows = RestConnector::new(notion_spec(&cfg))
            .fetch("users", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
