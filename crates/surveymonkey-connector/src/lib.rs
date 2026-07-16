//! # surveymonkey-connector
//!
//! The SurveyMonkey connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! SurveyMonkey REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! SurveyMonkey wraps its list responses in a `{ "data": [...] }` envelope and
//! paginates with `?page=N&per_page=M` (1-based). Requests authenticate with a
//! bearer token.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One SurveyMonkey target. `base_url` supports tests and alternate hosts;
/// `token` authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct SurveymonkeyConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the SurveyMonkey source spec for a config. This is the *entire*
/// connector: endpoints, columns, and pagination, as data.
pub fn surveymonkey_spec(cfg: &SurveymonkeyConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // SurveyMonkey wraps records in `{ "data": [...] }`.
    let data = || RowPath::Pointer {
        pointer: "/data".to_string(),
    };
    // SurveyMonkey paginates with ?page=N&per_page=M (1-based).
    let page = || Pagination::Page {
        page_param: "page".to_string(),
        size_param: "per_page".to_string(),
        page_size: 50,
        start_page: 1,
    };

    SourceSpec {
        name: "surveymonkey".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "surveys".to_string(),
                path: "/surveys".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("title", "title", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "contacts".to_string(),
                path: "/contacts".to_string(),
                row_path: data(),
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("email", "email", DataType::Text),
                    col("first_name", "first_name", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "groups".to_string(),
                path: "/groups".to_string(),
                row_path: data(),
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

    #[test]
    fn spec_exposes_expected_tables_and_bearer_auth() {
        let cfg = SurveymonkeyConfig {
            base_url: "https://api.surveymonkey.com/v3".to_string(),
            token: "sm_token".to_string(),
        };
        let spec = surveymonkey_spec(&cfg);
        assert_eq!(spec.name, "surveymonkey");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("surveys").is_some());
        assert!(spec.table("contacts").is_some());
        assert!(spec.table("groups").is_some());
        assert_eq!(spec.table("surveys").unwrap().columns.len(), 2);
        assert_eq!(spec.table("contacts").unwrap().columns.len(), 3);
        assert_eq!(spec.table("groups").unwrap().columns.len(), 2);
        assert!(matches!(
            spec.table("surveys").unwrap().row_path,
            RowPath::Pointer { .. }
        ));
        assert!(matches!(
            spec.table("surveys").unwrap().pagination,
            Pagination::Page { page_size: 50, .. }
        ));
        assert!(spec.table("surveys").unwrap().filters.is_empty());
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_contacts_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/contacts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": "1", "email": "a@example.com", "first_name": "Ada"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = SurveymonkeyConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(surveymonkey_spec(&cfg))
            .fetch("contacts", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
