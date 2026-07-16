//! # okta-connector
//!
//! The Okta connector for BudBuk. Like the other REST connectors, it is mostly
//! *config*: it builds a [`rest_connector::SourceSpec`] describing a few Okta
//! Core API collections and runs on the shared `RestConnector` engine (caching,
//! tracing, pushdown, FDW — all for free).
//!
//! Okta authenticates with an API token sent as `Authorization: SSWS <token>`.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Okta target. `base_url` is the org URL (e.g. `https://acme.okta.com`);
/// `token` authenticates as an SSWS API token.
#[derive(Debug, Clone)]
pub struct OktaConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Okta source spec for a config. This is the *entire* connector:
/// endpoints, columns, and auth, as data.
pub fn okta_spec(cfg: &OktaConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "okta".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::ApiKeyHeader {
            header: "Authorization".to_string(),
            value: format!("SSWS {}", cfg.token),
        },
        tables: vec![
            TableSpec {
                name: "users".to_string(),
                path: "/users".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("status", "status", DataType::Text),
                    col("created", "created", DataType::Timestamp),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "groups".to_string(),
                path: "/groups".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("created", "created", DataType::Timestamp),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "apps".to_string(),
                path: "/apps".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("label", "label", DataType::Text),
                    col("status", "status", DataType::Text),
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
    fn spec_exposes_expected_tables_and_ssws_auth() {
        let cfg = OktaConfig {
            base_url: "https://acme.okta.com".to_string(),
            token: "tok123".to_string(),
        };
        let spec = okta_spec(&cfg);
        assert_eq!(spec.name, "okta");
        assert_eq!(spec.base_url, "https://acme.okta.com");
        assert!(matches!(
            &spec.auth,
            AuthSpec::ApiKeyHeader { header, value }
                if header == "Authorization" && value == "SSWS tok123"
        ));
        assert!(spec.table("users").is_some());
        assert!(spec.table("groups").is_some());
        assert!(spec.table("apps").is_some());
        assert_eq!(spec.table("users").unwrap().columns.len(), 3);
        assert_eq!(spec.table("groups").unwrap().columns.len(), 2);
        assert_eq!(spec.table("apps").unwrap().columns.len(), 4);
        assert_eq!(spec.table("users").unwrap().path, "/users");
        assert_eq!(spec.table("groups").unwrap().path, "/groups");
        assert_eq!(spec.table("apps").unwrap().path, "/apps");
        for name in ["users", "groups", "apps"] {
            let t = spec.table(name).unwrap();
            assert!(matches!(t.row_path, RowPath::Root));
            assert!(matches!(t.pagination, Pagination::None));
            assert!(t.filters.is_empty());
        }
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
                {"id":"00u1","status":"ACTIVE","created":"2020-01-01T00:00:00.000Z"}
            ])))
            .mount(&server)
            .await;
        let cfg = OktaConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(okta_spec(&cfg))
            .fetch("users", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
