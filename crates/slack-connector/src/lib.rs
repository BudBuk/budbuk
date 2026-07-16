//! # slack-connector
//!
//! The Slack connector for BudBuk. Like the other REST connectors, it is mostly
//! *config*: it builds a [`rest_connector::SourceSpec`] describing a few Slack
//! Web API (`*.list`) collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! Slack's Web API authenticates with a bearer token (an OAuth or bot token).

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Slack target. `base_url` points at the Slack API (or a test server) and
/// `token` authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct SlackConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Slack source spec for a config. This is the *entire* connector:
/// endpoints, columns, and row locations, as data.
pub fn slack_spec(cfg: &SlackConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "slack".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "users".to_string(),
                path: "/users.list".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/members".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("real_name", "real_name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "channels".to_string(),
                path: "/conversations.list".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/channels".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("is_private", "is_private", DataType::Bool),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "usergroups".to_string(),
                path: "/usergroups.list".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/usergroups".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("handle", "handle", DataType::Text),
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
        let cfg = SlackConfig {
            base_url: "https://slack.com/api".to_string(),
            token: "xoxb-123".to_string(),
        };
        let spec = slack_spec(&cfg);
        assert_eq!(spec.name, "slack");
        assert_eq!(spec.base_url, "https://slack.com/api");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));

        let users = spec.table("users").unwrap();
        assert_eq!(users.path, "/users.list");
        assert!(matches!(&users.row_path, RowPath::Pointer { pointer } if pointer == "/members"));
        assert_eq!(users.columns.len(), 3);
        assert!(matches!(users.pagination, Pagination::None));
        assert!(users.filters.is_empty());

        let channels = spec.table("channels").unwrap();
        assert_eq!(channels.path, "/conversations.list");
        assert!(
            matches!(&channels.row_path, RowPath::Pointer { pointer } if pointer == "/channels")
        );
        assert_eq!(channels.columns.len(), 3);
        assert!(matches!(channels.columns[2].data_type, DataType::Bool));

        let usergroups = spec.table("usergroups").unwrap();
        assert_eq!(usergroups.path, "/usergroups.list");
        assert!(
            matches!(&usergroups.row_path, RowPath::Pointer { pointer } if pointer == "/usergroups")
        );
        assert_eq!(usergroups.columns.len(), 3);
        assert_eq!(usergroups.columns[2].name, "handle");

        assert!(spec.table("missing").is_none());

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_users_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users.list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "members": [
                    {"id":"U1","name":"alice","real_name":"Alice A"},
                    {"id":"U2","name":"bob","real_name":"Bob B"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = SlackConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(slack_spec(&cfg))
            .fetch("users", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
