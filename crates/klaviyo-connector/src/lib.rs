//! BudBuk connector for Klaviyo.
//!
//! Klaviyo's API is JSON:API: list responses look like
//! `{"data": [ {"id": "...", "attributes": {...}} ]}`, so rows are extracted
//! with [`RowPath::Pointer`] at `/data` and columns reach into nested
//! `attributes.*` fields. Authentication needs two static headers
//! (`Authorization: Klaviyo-API-Key <key>` and a `revision` date), so the spec
//! uses [`AuthSpec::Headers`].

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// Configuration for a Klaviyo source.
///
/// `base_url` is normally `"https://a.klaviyo.com/api"`; `api_key` is a private
/// API key sent as `Authorization: Klaviyo-API-Key <api_key>`.
#[derive(Debug, Clone)]
pub struct KlaviyoConfig {
    pub base_url: String,
    pub api_key: String,
}

/// Build the [`SourceSpec`] describing the Klaviyo JSON:API endpoints.
pub fn klaviyo_spec(cfg: &KlaviyoConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // Every Klaviyo list response nests records under the top-level `data` array.
    let data_rows = || RowPath::Pointer {
        pointer: "/data".to_string(),
    };

    let profiles = TableSpec {
        name: "profiles".to_string(),
        path: "/profiles".to_string(),
        row_path: data_rows(),
        columns: vec![
            col("id", "id", DataType::Text),
            col("email", "attributes.email", DataType::Text),
        ],
        pagination: Pagination::None,
        filters: vec![],
    };

    let lists = TableSpec {
        name: "lists".to_string(),
        path: "/lists".to_string(),
        row_path: data_rows(),
        columns: vec![
            col("id", "id", DataType::Text),
            col("name", "attributes.name", DataType::Text),
        ],
        pagination: Pagination::None,
        filters: vec![],
    };

    let campaigns = TableSpec {
        name: "campaigns".to_string(),
        path: "/campaigns".to_string(),
        row_path: data_rows(),
        columns: vec![
            col("id", "id", DataType::Text),
            col("name", "attributes.name", DataType::Text),
            col("status", "attributes.status", DataType::Text),
        ],
        pagination: Pagination::None,
        filters: vec![],
    };

    SourceSpec {
        name: "klaviyo".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Headers {
            headers: vec![
                (
                    "Authorization".to_string(),
                    format!("Klaviyo-API-Key {}", cfg.api_key),
                ),
                ("revision".to_string(), "2024-10-15".to_string()),
            ],
        },
        tables: vec![profiles, lists, campaigns],
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
        klaviyo_spec(&KlaviyoConfig {
            base_url: "https://a.klaviyo.com/api".to_string(),
            api_key: "pk_secret".to_string(),
        })
    }

    #[test]
    fn spec_has_expected_shape_and_header_auth() {
        let cfg = KlaviyoConfig {
            base_url: "https://a.klaviyo.com/api".to_string(),
            api_key: "pk_secret".to_string(),
        };
        let s = klaviyo_spec(&cfg);
        assert_eq!(s.name, "klaviyo");
        assert_eq!(s.base_url, "https://a.klaviyo.com/api");

        // Auth carries both static headers.
        assert!(matches!(
            &s.auth,
            AuthSpec::Headers { headers }
                if headers.len() == 2
                    && headers[0] == ("Authorization".to_string(), "Klaviyo-API-Key pk_secret".to_string())
                    && headers[1] == ("revision".to_string(), "2024-10-15".to_string())
        ));

        // Three tables, each reading rows from the JSON:API `/data` array.
        assert_eq!(s.tables.len(), 3);
        let names: Vec<&str> = s.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["profiles", "lists", "campaigns"]);
        for (t, expected_path) in s.tables.iter().zip(["/profiles", "/lists", "/campaigns"]) {
            assert_eq!(t.path, expected_path);
            assert!(matches!(&t.row_path, RowPath::Pointer { pointer } if pointer == "/data"));
            assert!(matches!(t.pagination, Pagination::None));
            assert!(t.filters.is_empty());
        }

        // Column shapes, including the nested `attributes.*` fields.
        let profiles = &s.tables[0];
        assert_eq!(profiles.columns.len(), 2);
        assert_eq!(profiles.columns[1].field, "attributes.email");
        let lists = &s.tables[1];
        assert_eq!(lists.columns.len(), 2);
        assert_eq!(lists.columns[1].field, "attributes.name");
        let campaigns = &s.tables[2];
        assert_eq!(campaigns.columns.len(), 3);
        assert_eq!(campaigns.columns[2].field, "attributes.status");

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_profiles_reads_nested_attributes_and_sends_auth_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/profiles"))
            .and(header("Authorization", "Klaviyo-API-Key pk_secret"))
            .and(header("revision", "2024-10-15"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": "01H", "attributes": {"email": "jane@example.com"}}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = KlaviyoConfig {
            base_url: server.uri(),
            api_key: "pk_secret".into(),
        };
        let rows = RestConnector::new(klaviyo_spec(&cfg))
            .fetch("profiles", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        // Column 0 is `id`, column 1 is the nested `attributes.email`.
        assert_eq!(rows[0].0[0].to_display_string(), "01H");
        assert_eq!(rows[0].0[1].to_display_string(), "jane@example.com");

        // Silence the unused `spec()` helper by exercising it too.
        assert_eq!(spec().tables.len(), 3);
    }
}
