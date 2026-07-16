//! # gdrive-connector
//!
//! The Google Drive connector for BudBuk. Like the other REST connectors, it is
//! mostly *config*: it builds a [`rest_connector::SourceSpec`] describing a few
//! Google Drive REST collections and runs on the shared `RestConnector` engine
//! (caching, tracing, pushdown, FDW — all for free).
//!
//! It authenticates with an OAuth bearer token.

use connector_sdk::DataType;
use rest_connector::{AuthSpec, ColumnSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// One Google Drive target. `base_url` supports tests and custom endpoints;
/// `token` authenticates as a bearer token.
#[derive(Debug, Clone)]
pub struct GdriveConfig {
    pub base_url: String,
    pub token: String,
}

/// Build the Google Drive source spec for a config. This is the *entire*
/// connector: endpoints, columns, pagination, and pushdown, as data.
pub fn gdrive_spec(cfg: &GdriveConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };

    SourceSpec {
        name: "gdrive".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "files".to_string(),
                path: "/files".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/files".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
                    col("mimeType", "mimeType", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
            TableSpec {
                name: "drives".to_string(),
                path: "/drives".to_string(),
                row_path: RowPath::Pointer {
                    pointer: "/drives".to_string(),
                },
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("name", "name", DataType::Text),
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
        let cfg = GdriveConfig {
            base_url: "https://www.googleapis.com/drive/v3".to_string(),
            token: "ya29_x".to_string(),
        };
        let spec = gdrive_spec(&cfg);
        assert_eq!(spec.name, "gdrive");
        assert_eq!(spec.base_url, "https://www.googleapis.com/drive/v3");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));

        let files = spec.table("files").unwrap();
        assert_eq!(files.path, "/files");
        assert!(matches!(&files.row_path, RowPath::Pointer { pointer } if pointer == "/files"));
        assert!(matches!(files.pagination, Pagination::None));
        assert!(files.filters.is_empty());
        assert_eq!(files.columns.len(), 3);
        assert_eq!(files.columns[2].name, "mimeType");

        let drives = spec.table("drives").unwrap();
        assert_eq!(drives.path, "/drives");
        assert!(matches!(&drives.row_path, RowPath::Pointer { pointer } if pointer == "/drives"));
        assert!(matches!(drives.pagination, Pagination::None));
        assert!(drives.filters.is_empty());
        assert_eq!(drives.columns.len(), 2);

        assert!(spec.table("missing").is_none());

        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }

    #[tokio::test]
    async fn fetch_files_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "files": [
                    {"id":"1","name":"doc","mimeType":"application/pdf"}
                ]
            })))
            .mount(&server)
            .await;
        let cfg = GdriveConfig {
            base_url: server.uri(),
            token: "t".into(),
        };
        let rows = RestConnector::new(gdrive_spec(&cfg))
            .fetch("files", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
