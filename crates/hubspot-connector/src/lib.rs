//! # hubspot-connector
//!
//! The HubSpot connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! HubSpot CRM (`/crm/v3`) object collections and runs on the shared
//! `RestConnector` engine (caching, tracing, pushdown, FDW — all for free).
//!
//! HubSpot's CRM API authenticates with a private-app access token sent as a
//! bearer token.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One HubSpot target. `base_url` points at the HubSpot API (or a test mock);
/// `token` is a private-app access token used as a bearer token.
#[derive(Debug, Clone)]
pub struct HubspotConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the HubSpot source spec for a config. This is the *entire* connector:
/// endpoints, columns, and typing, as data.
pub fn hubspot_spec(cfg: &HubspotConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // HubSpot CRM object endpoints return records under `/results` and share
    // the same base envelope, so build each table from its name and path.
    let object = |name: &str, path: &str| TableSpec {
        name: name.to_string(),
        path: path.to_string(),
        row_path: RowPath::Pointer {
            pointer: "/results".to_string(),
        },
        columns: vec![
            col("id", "id", DataType::Text),
            col("createdAt", "createdAt", DataType::Timestamp),
            col("updatedAt", "updatedAt", DataType::Timestamp),
            col("archived", "archived", DataType::Bool),
        ],
        pagination: Pagination::None,
        filters: vec![],
    };

    SourceSpec {
        name: "hubspot".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            object("contacts", "/crm/v3/objects/contacts"),
            object("companies", "/crm/v3/objects/companies"),
            object("deals", "/crm/v3/objects/deals"),
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
        let cfg = HubspotConfig {
            base_url: "https://api.hubapi.com".to_string(),
            token: "pat-na1-x".to_string(),
        };
        let spec = hubspot_spec(&cfg);
        assert_eq!(spec.name, "hubspot");
        assert_eq!(spec.base_url, "https://api.hubapi.com");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        for table in ["contacts", "companies", "deals"] {
            let t = spec.table(table).unwrap();
            assert_eq!(t.columns.len(), 4);
            assert!(matches!(t.row_path, RowPath::Pointer { .. }));
            assert!(matches!(t.pagination, Pagination::None));
            assert!(t.filters.is_empty());
        }
        assert_eq!(
            spec.table("contacts").unwrap().path,
            "/crm/v3/objects/contacts"
        );
        assert_eq!(
            spec.table("companies").unwrap().path,
            "/crm/v3/objects/companies"
        );
        assert_eq!(spec.table("deals").unwrap().path, "/crm/v3/objects/deals");
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_contacts_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/crm/v3/objects/contacts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    {"id":"1","createdAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-02T00:00:00Z","archived":false}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = HubspotConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(hubspot_spec(&cfg))
            .fetch("contacts", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
