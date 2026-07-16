//! # calendly-connector
//!
//! The Calendly connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Calendly REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! Calendly authenticates with a personal access token as a bearer token, and
//! wraps list responses in a `collection` array.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Calendly target. `base_url` points at the API (or a test server);
/// `token` authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct CalendlyConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Calendly source spec for a config. This is the *entire* connector:
/// endpoints, columns, and row extraction, as data.
pub fn calendly_spec(cfg: &CalendlyConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Calendly wraps every list response in a top-level `collection` array.
    let collection = || RowPath::Pointer {
        pointer: "/collection".to_string(),
    };

    SourceSpec {
        name: "calendly".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "event_types".to_string(),
                path: "/event_types".to_string(),
                row_path: collection(),
                columns: vec![
                    col("uri", "uri", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("active", "active", DataType::Bool),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "scheduled_events".to_string(),
                path: "/scheduled_events".to_string(),
                row_path: collection(),
                columns: vec![
                    col("uri", "uri", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("status", "status", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "organization_memberships".to_string(),
                path: "/organization_memberships".to_string(),
                row_path: collection(),
                columns: vec![
                    col("uri", "uri", DataType::Text),
                    col("role", "role", DataType::Text),
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
        let cfg = CalendlyConfig {
            base_url: "https://api.calendly.com".to_string(),
            token: "cal_tok".to_string(),
        };
        let spec = calendly_spec(&cfg);
        assert_eq!(spec.name, "calendly");
        assert_eq!(spec.base_url, "https://api.calendly.com");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));

        assert!(spec.table("event_types").is_some());
        assert!(spec.table("scheduled_events").is_some());
        assert!(spec.table("organization_memberships").is_some());

        assert_eq!(spec.table("event_types").unwrap().columns.len(), 3);
        assert_eq!(spec.table("scheduled_events").unwrap().columns.len(), 3);
        assert_eq!(
            spec.table("organization_memberships")
                .unwrap()
                .columns
                .len(),
            2
        );

        // Every table reads rows from the `/collection` pointer with no
        // pagination and no filters.
        for name in [
            "event_types",
            "scheduled_events",
            "organization_memberships",
        ] {
            let t = spec.table(name).unwrap();
            assert!(matches!(
                &t.row_path,
                RowPath::Pointer { pointer } if pointer == "/collection"
            ));
            assert!(matches!(t.pagination, Pagination::None));
            assert!(t.filters.is_empty());
        }

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_event_types_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/event_types"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "collection": [
                    {"uri": "https://api.calendly.com/event_types/1", "name": "30 min", "active": true},
                    {"uri": "https://api.calendly.com/event_types/2", "name": "60 min", "active": false}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = CalendlyConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(calendly_spec(&cfg))
            .fetch("event_types", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
