//! # mailchimp-connector
//!
//! The Mailchimp connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Mailchimp Marketing API collections and runs on the shared `RestConnector`
//! engine (caching, tracing, pushdown, FDW — all for free).
//!
//! Mailchimp authenticates with HTTP Basic auth: any username plus the API key
//! as the password.

use connector_sdk::DataType;
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec,
};

/// One Mailchimp target. `base_url` points at the data-center-specific API host
/// (e.g. `https://us1.api.mailchimp.com/3.0`); `api_key` is the API key used as
/// the Basic-auth password.
#[derive(Debug, Clone)]
pub struct MailchimpConfig {
    pub base_url: String,
    pub api_key: String,
}

/// Build the Mailchimp source spec for a config. This is the *entire*
/// connector: endpoints, columns, pagination, and pushdown, as data.
pub fn mailchimp_spec(cfg: &MailchimpConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Mailchimp paginates with ?offset=N&count=M.
    let offset = || Pagination::Offset {
        start_param: "offset".to_string(),
        limit_param: "count".to_string(),
        page_size: 100,
    };
    // Any username works; the API key is the password.
    let auth = AuthSpec::Basic {
        username: "budbuk".to_string(),
        password: cfg.api_key.clone(),
    };

    SourceSpec {
        name: "mailchimp".to_string(),
        base_url: cfg.base_url.clone(),
        auth,
        tables: vec![
            TableSpec {
                name: "lists".to_string(),
                path: "/lists".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/lists".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: offset(),
                filters: vec![],
            },
            TableSpec {
                name: "campaigns".to_string(),
                path: "/campaigns".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/campaigns".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("status", "status", DataType::Text),
                    col("type", "type", DataType::Text),
                ],
                pagination: offset(),
                filters: vec![FilterParam {
                    column: "status".to_string(),
                    param: "status".to_string(),
                }],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use connector_sdk::{Connector, Filter, Operator, Query, Value};
    use rest_connector::RestConnector;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn spec_exposes_expected_tables_and_basic_auth() {
        let cfg = MailchimpConfig {
            base_url: "https://us1.api.mailchimp.com/3.0".to_string(),
            api_key: "key-us1".to_string(),
        };
        let spec = mailchimp_spec(&cfg);
        assert_eq!(spec.name, "mailchimp");
        assert_eq!(spec.base_url, "https://us1.api.mailchimp.com/3.0");
        assert!(matches!(
            spec.auth,
            AuthSpec::Basic { ref username, .. } if username == "budbuk"
        ));
        let lists = spec.table("lists").unwrap();
        assert_eq!(lists.columns.len(), 2);
        assert!(lists.filters.is_empty());
        assert!(matches!(lists.row_path, RowPath::Pointer { ref pointer } if pointer == "/lists"));
        assert!(matches!(
            lists.pagination,
            Pagination::Offset { ref start_param, ref limit_param, page_size }
                if start_param == "offset" && limit_param == "count" && page_size == 100
        ));
        let campaigns = spec.table("campaigns").unwrap();
        assert_eq!(campaigns.columns.len(), 3);
        assert_eq!(campaigns.filters[0].column, "status");
        assert_eq!(campaigns.filters[0].param, "status");
        assert!(
            matches!(campaigns.row_path, RowPath::Pointer { ref pointer } if pointer == "/campaigns")
        );
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_lists_from_mock() {
        let server = MockServer::start().await;
        // Return fewer rows than page_size so offset pagination terminates.
        Mock::given(method("GET"))
            .and(path("/lists"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "lists": [
                    {"id": "abc", "name": "Weekly"},
                    {"id": "def", "name": "Monthly"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = MailchimpConfig {
            base_url: server.uri(),
            api_key: "k".into(),
        };
        let rows = RestConnector::new(mailchimp_spec(&cfg))
            .fetch("lists", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn fetch_campaigns_pushes_down_status_filter() {
        let server = MockServer::start().await;
        // Only respond when the status filter is pushed down as a query param;
        // a single short page terminates offset pagination.
        Mock::given(method("GET"))
            .and(path("/campaigns"))
            .and(query_param("status", "sent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "campaigns": [
                    {"id": "c1", "status": "sent", "type": "regular"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = MailchimpConfig {
            base_url: server.uri(),
            api_key: "k".into(),
        };
        let query = Query {
            filters: vec![Filter::new(
                "status",
                Operator::Eq,
                Value::Text("sent".into()),
            )],
            ..Default::default()
        };
        let rows = RestConnector::new(mailchimp_spec(&cfg))
            .fetch("campaigns", &query)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
