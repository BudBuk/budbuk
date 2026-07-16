//! # servicenow-connector
//!
//! The ServiceNow connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! ServiceNow Table API (`/table/<name>`) collections and runs on the shared
//! `RestConnector` engine (caching, tracing, pushdown, FDW — all for free).
//!
//! ServiceNow's Table API paginates with `?sysparm_offset=N&sysparm_limit=M`
//! and wraps records in a top-level `result` array. It authenticates with HTTP
//! Basic auth (username/password).

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One ServiceNow target. `base_url` points at an instance (e.g.
/// `https://dev12345.service-now.com/api/now`); `username`/`password`
/// authenticate via HTTP Basic auth.
#[derive(Debug, Clone)]
pub struct ServiceNowConfig {
    pub base_url: String,
    pub username: String,
    pub password: String,
}

/// Build the ServiceNow source spec for a config. This is the *entire*
/// connector: endpoints, columns, pagination, and pushdown, as data.
pub fn servicenow_spec(cfg: &ServiceNowConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // ServiceNow Table API: records live in `/result`, offset/limit paged.
    let result_rows = || RowPath::Pointer {
        pointer: "/result".to_string(),
    };
    let offset = || Pagination::Offset {
        start_param: "sysparm_offset".to_string(),
        limit_param: "sysparm_limit".to_string(),
        page_size: 100,
    };
    // Every table exposes the same four core fields.
    let core_cols = || {
        vec![
            col("sys_id", "sys_id", DataType::Text),
            col("number", "number", DataType::Text),
            col("short_description", "short_description", DataType::Text),
            col("state", "state", DataType::Text),
        ]
    };

    SourceSpec {
        name: "servicenow".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Basic {
            username: cfg.username.clone(),
            password: cfg.password.clone(),
        },
        tables: vec![
            TableSpec {
                name: "incident".to_string(),
                path: "/table/incident".to_string(),
                row_path: result_rows(),
                columns: core_cols(),
                pagination: offset(),
                filters: vec![],
            },
            TableSpec {
                name: "problem".to_string(),
                path: "/table/problem".to_string(),
                row_path: result_rows(),
                columns: core_cols(),
                pagination: offset(),
                filters: vec![],
            },
            TableSpec {
                name: "change_request".to_string(),
                path: "/table/change_request".to_string(),
                row_path: result_rows(),
                columns: core_cols(),
                pagination: offset(),
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

    fn cfg(base_url: String) -> ServiceNowConfig {
        ServiceNowConfig {
            base_url,
            username: "admin".to_string(),
            password: "secret".to_string(),
        }
    }

    #[test]
    fn spec_exposes_expected_tables_and_basic_auth() {
        let cfg = cfg("https://dev12345.service-now.com/api/now".to_string());
        let spec = servicenow_spec(&cfg);
        assert_eq!(spec.name, "servicenow");
        assert_eq!(spec.base_url, "https://dev12345.service-now.com/api/now");
        assert!(matches!(spec.auth, AuthSpec::Basic { .. }));
        for name in ["incident", "problem", "change_request"] {
            let table = spec.table(name).unwrap();
            assert_eq!(table.columns.len(), 4);
            assert!(table.filters.is_empty());
            assert!(matches!(
                &table.row_path,
                RowPath::Pointer { pointer } if pointer == "/result"
            ));
            assert!(matches!(
                &table.pagination,
                Pagination::Offset {
                    start_param,
                    limit_param,
                    page_size: 100,
                } if start_param == "sysparm_offset" && limit_param == "sysparm_limit"
            ));
        }
        assert_eq!(spec.table("incident").unwrap().path, "/table/incident");
        assert_eq!(spec.table("problem").unwrap().path, "/table/problem");
        assert_eq!(
            spec.table("change_request").unwrap().path,
            "/table/change_request"
        );
        assert!(spec.table("missing").is_none());
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_incidents_from_mock() {
        let server = MockServer::start().await;
        // Return fewer rows than page_size (100) so offset pagination stops.
        Mock::given(method("GET"))
            .and(path("/table/incident"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": [
                    {
                        "sys_id": "abc123",
                        "number": "INC0001",
                        "short_description": "Email is down",
                        "state": "1"
                    },
                    {
                        "sys_id": "def456",
                        "number": "INC0002",
                        "short_description": "Printer jammed",
                        "state": "2"
                    }
                ]
            })))
            .mount(&server)
            .await;

        let cfg = cfg(server.uri());
        let rows = RestConnector::new(servicenow_spec(&cfg))
            .fetch("incident", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
