//! # pagerduty-connector
//!
//! The PagerDuty connector for BudBuk. It builds a
//! [`rest_connector::SourceSpec`] describing a few PagerDuty REST collections
//! (incidents, services, users) and runs on the shared `RestConnector` engine
//! (HTTP, offset pagination, pushdown, caching — all for free).
//!
//! PagerDuty authenticates with an `Authorization: Token token=<key>` header and
//! wraps each collection's array in a named key (e.g. `{"incidents": [...]}`),
//! so rows are read via a JSON pointer.

use connector_sdk::DataType;
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec,
};

/// One PagerDuty account. `api_key` authenticates; `base_url` is kept as a field
/// so tests can point it at a mock server.
#[derive(Debug, Clone)]
pub struct PagerDutyConfig {
    pub base_url: String,
    pub api_key: String,
}

impl PagerDutyConfig {
    /// A config for the production PagerDuty API, authenticated with `api_key`.
    pub fn new(api_key: &str) -> Self {
        Self {
            base_url: "https://api.pagerduty.com".to_string(),
            api_key: api_key.to_string(),
        }
    }
}

/// Build the PagerDuty source spec for a config. This is the *entire* connector:
/// endpoints, columns, and pagination, as data.
pub fn pagerduty_spec(cfg: &PagerDutyConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // All PagerDuty collections paginate with ?offset=N&limit=M.
    let offset = || Pagination::Offset {
        start_param: "offset".to_string(),
        limit_param: "limit".to_string(),
        page_size: 25,
    };
    let table = |name: &str, columns: Vec<ColumnSpec>| TableSpec {
        name: name.to_string(),
        path: format!("/{name}"),
        row_path: RowPath::Pointer {
            pointer: format!("/{name}"),
        },
        columns,
        pagination: offset(),
        filters: Vec::<FilterParam>::new(),
    };

    SourceSpec {
        name: "pagerduty".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::ApiKeyHeader {
            header: "Authorization".to_string(),
            value: format!("Token token={}", cfg.api_key),
        },
        tables: vec![
            table(
                "incidents",
                vec![
                    col("id", "id", DataType::Text),
                    col("incident_number", "incident_number", DataType::Integer),
                    col("title", "title", DataType::Text),
                    col("status", "status", DataType::Text),
                    col("urgency", "urgency", DataType::Text),
                    col("created_at", "created_at", DataType::Timestamp),
                ],
            ),
            table(
                "services",
                vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("status", "status", DataType::Text),
                    col("created_at", "created_at", DataType::Timestamp),
                ],
            ),
            table(
                "users",
                vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("email", "email", DataType::Text),
                    col("role", "role", DataType::Text),
                ],
            ),
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
    fn spec_describes_pagerduty_collections() {
        let cfg = PagerDutyConfig::new("secret");
        let spec = pagerduty_spec(&cfg);

        assert_eq!(spec.name, "pagerduty");
        assert_eq!(spec.base_url, "https://api.pagerduty.com");
        assert_eq!(spec.tables.len(), 3);

        for (name, cols) in [("incidents", 6), ("services", 4), ("users", 4)] {
            let t = spec.tables.iter().find(|t| t.name == name).unwrap();
            assert_eq!(t.path, format!("/{name}"));
            assert!(
                matches!(&t.row_path, RowPath::Pointer { pointer } if pointer == &format!("/{name}"))
            );
            assert_eq!(t.columns.len(), cols, "column count for {name}");
            assert!(t.filters.is_empty());
            assert!(matches!(
                t.pagination,
                Pagination::Offset {
                    ref start_param,
                    ref limit_param,
                    page_size: 25,
                } if start_param == "offset" && limit_param == "limit"
            ));
        }

        assert!(matches!(
            &spec.auth,
            AuthSpec::ApiKeyHeader { header, value }
                if header == "Authorization" && value.starts_with("Token token=")
        ));
    }

    #[tokio::test]
    async fn fetch_incidents_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/incidents"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "incidents": [{"id":"PABC","incident_number":42,"title":"down","status":"triggered"}]
            })))
            .mount(&server)
            .await;
        let cfg = PagerDutyConfig {
            base_url: server.uri(),
            api_key: "k".into(),
        };
        let rows = RestConnector::new(pagerduty_spec(&cfg))
            .fetch("incidents", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
