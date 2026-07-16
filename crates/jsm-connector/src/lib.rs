//! # jsm-connector
//!
//! The Jira Service Management (JSM) connector for BudBuk. Like the other REST
//! connectors, it is mostly *config*: it builds a [`rest_connector::SourceSpec`]
//! describing a few JSM REST collections and runs on the shared `RestConnector`
//! engine (caching, tracing, pushdown, FDW — all for free).
//!
//! JSM authenticates with Atlassian HTTP Basic auth: your account email as the
//! username and an API token as the password.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One JSM target. `base_url` is the site's REST base; `email` + `api_token`
/// form the Atlassian Basic-auth credential pair.
#[derive(Debug, Clone)]
pub struct JsmConfig {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
}

/// Build the JSM source spec for a config. This is the *entire* connector:
/// endpoints, columns, pagination, and auth, as data.
pub fn jsm_spec(cfg: &JsmConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // JSM paginates with ?start=N&limit=M and wraps rows under `/values`.
    let page = || Pagination::Offset {
        start_param: "start".to_string(),
        limit_param: "limit".to_string(),
        page_size: 50,
    };
    let rows = || RowPath::Pointer {
        pointer: "/values".to_string(),
    };

    SourceSpec {
        name: "jsm".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Basic {
            username: cfg.email.clone(),
            password: cfg.api_token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "request".to_string(),
                path: "/request".to_string(),
                row_path: rows(),
                columns: vec![
                    col("issueId", "issueId", DataType::Text),
                    col("issueKey", "issueKey", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "servicedesk".to_string(),
                path: "/servicedesk".to_string(),
                row_path: rows(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("projectName", "projectName", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "organization".to_string(),
                path: "/organization".to_string(),
                row_path: rows(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                ],
                pagination: page(),
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

    fn config() -> JsmConfig {
        JsmConfig {
            base_url: "https://acme.atlassian.net/rest/servicedeskapi".to_string(),
            email: "agent@acme.com".to_string(),
            api_token: "tok_123".to_string(),
        }
    }

    #[test]
    fn spec_exposes_expected_tables_and_basic_auth() {
        let cfg = config();
        let spec = jsm_spec(&cfg);
        assert_eq!(spec.name, "jsm");
        assert_eq!(spec.base_url, cfg.base_url);
        assert!(matches!(spec.auth, AuthSpec::Basic { .. }));

        for (table, cols) in [("request", 2), ("servicedesk", 2), ("organization", 2)] {
            let t = spec.table(table).unwrap();
            assert_eq!(t.columns.len(), cols);
            assert!(t.filters.is_empty());
            assert!(matches!(&t.row_path, RowPath::Pointer { pointer } if pointer == "/values"));
            assert!(matches!(
                &t.pagination,
                Pagination::Offset { start_param, limit_param, page_size }
                    if start_param == "start" && limit_param == "limit" && *page_size == 50
            ));
        }
        assert!(spec.table("missing").is_none());
        assert_eq!(spec.table("request").unwrap().path, "/request");
        assert_eq!(spec.table("request").unwrap().columns[1].name, "issueKey");
        assert_eq!(
            spec.table("servicedesk").unwrap().columns[1].name,
            "projectName"
        );
        assert_eq!(spec.table("organization").unwrap().columns[1].name, "name");
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_servicedesks_from_mock() {
        let server = MockServer::start().await;
        // Return fewer rows than page_size (50) so offset pagination terminates.
        Mock::given(method("GET"))
            .and(path("/servicedesk"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "values": [
                    {"id": "10", "projectName": "IT Support"},
                    {"id": "11", "projectName": "HR Requests"}
                ]
            })))
            .mount(&server)
            .await;

        let cfg = JsmConfig {
            base_url: server.uri(),
            ..config()
        };
        let rows = RestConnector::new(jsm_spec(&cfg))
            .fetch("servicedesk", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
