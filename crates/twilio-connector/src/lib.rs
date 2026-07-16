//! # twilio-connector
//!
//! The Twilio connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Twilio REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! Twilio authenticates with HTTP Basic auth using the account SID as the
//! username and the auth token as the password.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Twilio target. `base_url` supports tests and regional hosts;
/// `account_sid`/`auth_token` are the HTTP Basic credentials.
#[derive(Debug, Clone)]
pub struct TwilioConfig {
    pub base_url: String,
    pub account_sid: String,
    pub auth_token: String,
}

/// Build the Twilio source spec for a config. This is the *entire* connector:
/// endpoints, columns, pagination, and pushdown, as data.
pub fn twilio_spec(cfg: &TwilioConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Twilio deprecated numbered paging (?Page=N now returns HTTP 400 error
    // 20001 — it requires cursor paging via AfterSid). Fetch a single page,
    // which is all the shared engine's pagination models support here.
    SourceSpec {
        name: "twilio".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Basic {
            username: cfg.account_sid.clone(),
            password: cfg.auth_token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "messages".to_string(),
                path: "/Messages.json".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/messages".to_string(),
                },
                columns: vec![
                    col("sid", "sid", DataType::Text),
                    col("status", "status", DataType::Text),
                    col("to", "to", DataType::Text),
                    col("from", "from", DataType::Text),
                    col("body", "body", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "calls".to_string(),
                path: "/Calls.json".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/calls".to_string(),
                },
                columns: vec![
                    col("sid", "sid", DataType::Text),
                    col("status", "status", DataType::Text),
                    col("to", "to", DataType::Text),
                    col("from", "from", DataType::Text),
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

    fn cfg(base_url: String) -> TwilioConfig {
        TwilioConfig {
            base_url,
            account_sid: "AC123".to_string(),
            auth_token: "secret".to_string(),
        }
    }

    #[test]
    fn spec_exposes_expected_tables_and_basic_auth() {
        let cfg = cfg("https://api.twilio.com".to_string());
        let spec = twilio_spec(&cfg);
        assert_eq!(spec.name, "twilio");
        assert_eq!(spec.base_url, "https://api.twilio.com");
        assert!(matches!(spec.auth, AuthSpec::Basic { .. }));

        let messages = spec.table("messages").unwrap();
        assert_eq!(messages.path, "/Messages.json");
        assert_eq!(messages.columns.len(), 5);
        assert!(messages.filters.is_empty());
        assert!(matches!(
            &messages.row_path,
            RowPath::Pointer { pointer } if pointer == "/messages"
        ));
        assert!(matches!(messages.pagination, Pagination::None));

        let calls = spec.table("calls").unwrap();
        assert_eq!(calls.path, "/Calls.json");
        assert_eq!(calls.columns.len(), 4);
        assert!(calls.filters.is_empty());
        assert!(matches!(
            &calls.row_path,
            RowPath::Pointer { pointer } if pointer == "/calls"
        ));

        assert!(spec.table("nope").is_none());

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_messages_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/Messages.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "messages": [
                    {"sid":"SM1","status":"delivered","to":"+15551112222","from":"+15553334444","body":"hi"}
                ]
            })))
            .mount(&server)
            .await;
        let rows = RestConnector::new(twilio_spec(&cfg(server.uri())))
            .fetch("messages", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn fetch_calls_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/Calls.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "calls": [
                    {"sid":"CA1","status":"completed","to":"+15551112222","from":"+15553334444"}
                ]
            })))
            .mount(&server)
            .await;
        let rows = RestConnector::new(twilio_spec(&cfg(server.uri())))
            .fetch("calls", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
