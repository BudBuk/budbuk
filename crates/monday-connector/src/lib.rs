//! BudBuk connector for Monday.com.
//!
//! Monday exposes a single GraphQL endpoint, so this is a [`GraphQlSpec`] over
//! the shared GraphQL engine (not the REST catalog). Auth is the API token in a
//! plain `Authorization` header (no `Bearer` prefix). Each table is a stored
//! GraphQL query returning a list of nodes.

use connector_sdk::DataType;
use graphql_connector::{AuthSpec, ColumnSpec, GraphQlSpec, GraphQlTable, NodeShape, Pagination};

/// One Monday.com target.
pub struct MondayConfig {
    /// GraphQL endpoint, defaults to `https://api.monday.com/v2`.
    pub base_url: String,
    /// A Monday API token.
    pub token: String,
}

fn col(name: &str, field: &str, data_type: DataType) -> ColumnSpec {
    ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    }
}

fn table(name: &str, query: &str, pointer: &str, columns: Vec<ColumnSpec>) -> GraphQlTable {
    GraphQlTable {
        name: name.to_string(),
        query: query.to_string(),
        data_pointer: pointer.to_string(),
        shape: NodeShape::List,
        columns,
        pagination: Pagination::None,
        filters: vec![],
    }
}

/// Build a [`GraphQlSpec`] for Monday.com.
pub fn monday_spec(cfg: &MondayConfig) -> GraphQlSpec {
    GraphQlSpec {
        name: "monday".to_string(),
        endpoint: cfg.base_url.clone(),
        auth: AuthSpec::ApiKeyHeader {
            header: "Authorization".to_string(),
            value: cfg.token.clone(),
        },
        tables: vec![
            table(
                "boards",
                "query { boards(limit: 100) { id name state } }",
                "/boards",
                vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("state", "state", DataType::Text),
                ],
            ),
            table(
                "users",
                "query { users { id name email } }",
                "/users",
                vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("email", "email", DataType::Text),
                ],
            ),
            table(
                "workspaces",
                "query { workspaces { id name kind } }",
                "/workspaces",
                vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("kind", "kind", DataType::Text),
                ],
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use connector_sdk::{Connector, Query};
    use graphql_connector::GraphQlConnector;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn spec_exposes_expected_tables_and_header_auth() {
        let spec = monday_spec(&MondayConfig {
            base_url: "https://api.monday.com/v2".to_string(),
            token: "tok".to_string(),
        });
        assert_eq!(spec.name, "monday");
        assert_eq!(spec.endpoint, "https://api.monday.com/v2");
        assert!(matches!(spec.auth, AuthSpec::ApiKeyHeader { .. }));
        assert!(spec.table("boards").is_some());
        assert!(spec.table("users").is_some());
        assert_eq!(spec.table("workspaces").unwrap().columns.len(), 3);
        assert!(spec.table("missing").is_none());
    }

    #[tokio::test]
    async fn fetches_boards_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {"boards": [{"id": "12", "name": "Sephora", "state": "active"}]}
            })))
            .mount(&server)
            .await;
        let spec = monday_spec(&MondayConfig {
            base_url: format!("{}/v2", server.uri()),
            token: "tok".to_string(),
        });
        let rows = GraphQlConnector::new(spec)
            .fetch("boards", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0[1].to_display_string(), "Sephora");
    }
}
