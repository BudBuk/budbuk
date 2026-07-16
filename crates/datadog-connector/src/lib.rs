//! BudBuk connector for Datadog.
//!
//! Exposes a few Datadog REST endpoints (`monitors`, `dashboards`, `users`) as
//! tables via a declarative [`SourceSpec`]. Datadog authenticates with *two*
//! static headers — `DD-API-KEY` and `DD-APPLICATION-KEY` — so this connector
//! uses [`AuthSpec::Headers`]. The `dashboards` and `users` endpoints nest their
//! records under `/dashboards` and `/data` respectively (extracted with
//! [`RowPath::Pointer`]), and `users` reads dotted `attributes.*` fields.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// Configuration for a Datadog source.
///
/// `base_url` looks like `"https://api.datadoghq.com/api"`. Both keys are
/// required and are sent on every request as static headers.
#[derive(Debug, Clone)]
pub struct DatadogConfig {
    pub base_url: String,
    pub api_key: String,
    pub app_key: String,
}

/// Build the [`SourceSpec`] describing the Datadog API.
pub fn datadog_spec(cfg: &DatadogConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    let monitors = TableSpec {
        name: "monitors".to_string(),
        path: "/v1/monitor".to_string(),
        row_path: RowPath::Root,
        columns: vec![
            col("id", "id", DataType::Integer),
            col("name", "name", DataType::Text),
            col("type", "type", DataType::Text),
        ],
        pagination: Pagination::None,
        filters: vec![],
    };

    let dashboards = TableSpec {
        name: "dashboards".to_string(),
        path: "/v1/dashboard".to_string(),
        row_path: RowPath::Pointer {
            pointer: "/dashboards".to_string(),
        },
        columns: vec![
            col("id", "id", DataType::Text),
            col("title", "title", DataType::Text),
        ],
        pagination: Pagination::None,
        filters: vec![],
    };

    let users = TableSpec {
        name: "users".to_string(),
        path: "/v2/users".to_string(),
        row_path: RowPath::Pointer {
            pointer: "/data".to_string(),
        },
        columns: vec![
            col("id", "id", DataType::Text),
            col("email", "attributes.email", DataType::Text),
            col("name", "attributes.name", DataType::Text),
        ],
        pagination: Pagination::None,
        filters: vec![],
    };

    SourceSpec {
        name: "datadog".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Headers {
            headers: vec![
                ("DD-API-KEY".to_string(), cfg.api_key.clone()),
                ("DD-APPLICATION-KEY".to_string(), cfg.app_key.clone()),
            ],
        },
        tables: vec![monitors, dashboards, users],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use connector_sdk::{Connector, Query};
    use rest_connector::RestConnector;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn spec() -> SourceSpec {
        datadog_spec(&DatadogConfig {
            base_url: "https://api.datadoghq.com/api".to_string(),
            api_key: "api-secret".to_string(),
            app_key: "app-secret".to_string(),
        })
    }

    #[test]
    fn spec_has_expected_shape_and_headers_auth() {
        let cfg = DatadogConfig {
            base_url: "https://api.datadoghq.com/api".to_string(),
            api_key: "api-secret".to_string(),
            app_key: "app-secret".to_string(),
        };
        let s = datadog_spec(&cfg);
        assert_eq!(s.name, "datadog");
        assert_eq!(s.base_url, "https://api.datadoghq.com/api");

        // Auth carries both static Datadog headers, in order.
        assert!(matches!(
            &s.auth,
            AuthSpec::Headers { headers }
                if headers.len() == 2
                    && headers[0] == ("DD-API-KEY".to_string(), "api-secret".to_string())
                    && headers[1]
                        == ("DD-APPLICATION-KEY".to_string(), "app-secret".to_string())
        ));

        // Three tables with the expected paths.
        assert_eq!(s.tables.len(), 3);
        let names: Vec<&str> = s.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["monitors", "dashboards", "users"]);

        // monitors: Root, three columns, no pagination, no filters.
        let monitors = s.table("monitors").unwrap();
        assert_eq!(monitors.path, "/v1/monitor");
        assert!(matches!(monitors.row_path, RowPath::Root));
        assert!(matches!(monitors.pagination, Pagination::None));
        assert!(monitors.filters.is_empty());
        assert_eq!(monitors.columns.len(), 3);
        assert_eq!(monitors.columns[0].field, "id");
        assert!(matches!(monitors.columns[0].data_type, DataType::Integer));
        assert!(matches!(monitors.columns[1].data_type, DataType::Text));

        // dashboards: Pointer "/dashboards", two Text columns.
        let dashboards = s.table("dashboards").unwrap();
        assert_eq!(dashboards.path, "/v1/dashboard");
        assert!(matches!(
            &dashboards.row_path,
            RowPath::Pointer { pointer } if pointer == "/dashboards"
        ));
        assert!(matches!(dashboards.pagination, Pagination::None));
        assert!(dashboards.filters.is_empty());
        assert_eq!(dashboards.columns.len(), 2);
        assert!(matches!(dashboards.columns[0].data_type, DataType::Text));

        // users: Pointer "/data", dotted attributes.* fields.
        let users = s.table("users").unwrap();
        assert_eq!(users.path, "/v2/users");
        assert!(matches!(
            &users.row_path,
            RowPath::Pointer { pointer } if pointer == "/data"
        ));
        assert!(matches!(users.pagination, Pagination::None));
        assert!(users.filters.is_empty());
        assert_eq!(users.columns.len(), 3);
        assert_eq!(users.columns[0].field, "id");
        assert_eq!(users.columns[1].name, "email");
        assert_eq!(users.columns[1].field, "attributes.email");
        assert_eq!(users.columns[2].name, "name");
        assert_eq!(users.columns[2].field, "attributes.name");

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_monitors_from_mock_sends_both_keys() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/monitor"))
            .and(header("DD-API-KEY", "api-secret"))
            .and(header("DD-APPLICATION-KEY", "app-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": 1, "name": "cpu", "type": "metric alert"}
            ])))
            .mount(&server)
            .await;
        let cfg = DatadogConfig {
            base_url: server.uri(),
            api_key: "api-secret".into(),
            app_key: "app-secret".into(),
        };
        let rows = RestConnector::new(datadog_spec(&cfg))
            .fetch("monitors", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0[1].to_display_string(), "cpu");
    }

    #[tokio::test]
    async fn fetch_users_reads_nested_attributes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/users"))
            .and(header("DD-API-KEY", "api-secret"))
            .and(header("DD-APPLICATION-KEY", "app-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": "user1",
                    "attributes": {"email": "a@b.com", "name": "Ada"}
                }]
            })))
            .mount(&server)
            .await;
        let _ = spec();
        let cfg = DatadogConfig {
            base_url: server.uri(),
            api_key: "api-secret".into(),
            app_key: "app-secret".into(),
        };
        let rows = RestConnector::new(datadog_spec(&cfg))
            .fetch("users", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0[0].to_display_string(), "user1");
        assert_eq!(rows[0].0[1].to_display_string(), "a@b.com");
        assert_eq!(rows[0].0[2].to_display_string(), "Ada");
    }
}
