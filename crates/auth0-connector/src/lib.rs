//! # auth0-connector
//!
//! The Auth0 connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Auth0 Management API collections and runs on the shared `RestConnector`
//! engine (caching, tracing, pushdown, FDW — all for free).
//!
//! It authenticates with a Management API access token (bearer).

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Auth0 tenant. `base_url` is the tenant's Management API base (e.g.
/// `https://tenant.us.auth0.com/api/v2`); `token` authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct Auth0Config {
    pub base_url: String,
    pub token: String,
}

/// Build the Auth0 source spec for a config. This is the *entire* connector:
/// endpoints, columns, and types, as data.
pub fn auth0_spec(cfg: &Auth0Config) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "auth0".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "users".to_string(),
                path: "/users".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("user_id", "user_id", DataType::Text),
                    col("email", "email", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "clients".to_string(),
                path: "/clients".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("client_id", "client_id", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "connections".to_string(),
                path: "/connections".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("strategy", "strategy", DataType::Text),
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
    use connector_sdk::{Connector, Query};
    use rest_connector::RestConnector;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn spec_exposes_expected_tables_and_bearer_auth() {
        let cfg = Auth0Config {
            base_url: "https://tenant.us.auth0.com/api/v2".to_string(),
            token: "mgmt_token".to_string(),
        };
        let spec = auth0_spec(&cfg);
        assert_eq!(spec.name, "auth0");
        assert_eq!(spec.base_url, "https://tenant.us.auth0.com/api/v2");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("users").is_some());
        assert!(spec.table("clients").is_some());
        assert!(spec.table("connections").is_some());
        assert_eq!(spec.table("users").unwrap().columns.len(), 3);
        assert_eq!(spec.table("clients").unwrap().columns.len(), 2);
        assert_eq!(spec.table("connections").unwrap().columns.len(), 3);
        assert_eq!(spec.table("users").unwrap().path, "/users");
        assert!(matches!(
            spec.table("users").unwrap().row_path,
            RowPath::Root
        ));
        assert!(matches!(
            spec.table("clients").unwrap().pagination,
            Pagination::None
        ));
        assert!(spec.table("connections").unwrap().filters.is_empty());
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_users_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"user_id":"auth0|1","email":"a@b.com","name":"Alice"}
            ])))
            .mount(&server)
            .await;
        let cfg = Auth0Config {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(auth0_spec(&cfg))
            .fetch("users", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
