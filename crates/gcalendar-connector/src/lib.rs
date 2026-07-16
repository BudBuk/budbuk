//! # gcalendar-connector
//!
//! The Google Calendar connector for BudBuk. Like the other REST connectors, it
//! is mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a
//! couple of Google Calendar API collections and runs on the shared
//! `RestConnector` engine (caching, tracing, pushdown, FDW — all for free).
//!
//! It authenticates with an OAuth bearer token.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Google Calendar target. `base_url` supports tests and alternate hosts;
/// `token` is the OAuth bearer token.
#[derive(Debug, Clone)]
pub struct GcalendarConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Google Calendar source spec for a config. This is the *entire*
/// connector: endpoints, columns, pagination, and pushdown, as data.
pub fn gcalendar_spec(cfg: &GcalendarConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "gcalendar".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "calendars".to_string(),
                path: "/users/me/calendarList".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/items".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("summary", "summary", DataType::Text),
                    col("timeZone", "timeZone", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "settings".to_string(),
                path: "/users/me/settings".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/items".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("value", "value", DataType::Text),
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
        let cfg = GcalendarConfig {
            base_url: "https://www.googleapis.com/calendar/v3".to_string(),
            token: "ya29.token".to_string(),
        };
        let spec = gcalendar_spec(&cfg);
        assert_eq!(spec.name, "gcalendar");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("calendars").is_some());
        assert!(spec.table("settings").is_some());
        assert_eq!(spec.table("calendars").unwrap().columns.len(), 3);
        assert_eq!(spec.table("settings").unwrap().columns.len(), 2);
        assert!(matches!(
            spec.table("calendars").unwrap().row_path,
            RowPath::Pointer { .. }
        ));
        assert!(matches!(
            spec.table("settings").unwrap().pagination,
            Pagination::None
        ));
        assert!(spec.table("calendars").unwrap().filters.is_empty());
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_calendars_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users/me/calendarList"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {"id": "primary", "summary": "Work", "timeZone": "UTC"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = GcalendarConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(gcalendar_spec(&cfg))
            .fetch("calendars", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
