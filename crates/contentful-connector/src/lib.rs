//! BudBuk connector for Contentful.
//!
//! Exposes Contentful's Content Delivery API as tables (`entries`, `assets`,
//! `content_types`) via a declarative [`SourceSpec`]. Records are wrapped in an
//! `items` array and nest metadata under `sys`, so rows are extracted with
//! [`RowPath::Pointer`] and columns read dotted `sys.*` fields.

use connector_sdk::DataType;
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec,
};

/// Configuration for a Contentful source.
///
/// `base_url` looks like
/// `"https://cdn.contentful.com/spaces/SPACE_ID/environments/master"`.
#[derive(Debug, Clone)]
pub struct ContentfulConfig {
    pub base_url: String,
    pub access_token: String,
}

/// Build the [`SourceSpec`] describing the Contentful Content Delivery API.
pub fn contentful_spec(cfg: &ContentfulConfig) -> SourceSpec {
    let pagination = || Pagination::Offset {
        start_param: "skip".to_string(),
        limit_param: "limit".to_string(),
        page_size: 100,
    };

    let entries = TableSpec {
        name: "entries".to_string(),
        path: "/entries".to_string(),
        row_path: RowPath::Pointer {
            pointer: "/items".to_string(),
        },
        columns: vec![
            ColumnSpec {
                name: "id".to_string(),
                field: "sys.id".to_string(),
                data_type: DataType::Text,
            },
            ColumnSpec {
                name: "content_type".to_string(),
                field: "sys.contentType.sys.id".to_string(),
                data_type: DataType::Text,
            },
            ColumnSpec {
                name: "created_at".to_string(),
                field: "sys.createdAt".to_string(),
                data_type: DataType::Timestamp,
            },
            ColumnSpec {
                name: "updated_at".to_string(),
                field: "sys.updatedAt".to_string(),
                data_type: DataType::Timestamp,
            },
        ],
        pagination: pagination(),
        filters: vec![FilterParam {
            column: "content_type".to_string(),
            param: "content_type".to_string(),
        }],
    };

    let assets = TableSpec {
        name: "assets".to_string(),
        path: "/assets".to_string(),
        row_path: RowPath::Pointer {
            pointer: "/items".to_string(),
        },
        columns: vec![
            ColumnSpec {
                name: "id".to_string(),
                field: "sys.id".to_string(),
                data_type: DataType::Text,
            },
            ColumnSpec {
                name: "created_at".to_string(),
                field: "sys.createdAt".to_string(),
                data_type: DataType::Timestamp,
            },
            ColumnSpec {
                name: "updated_at".to_string(),
                field: "sys.updatedAt".to_string(),
                data_type: DataType::Timestamp,
            },
        ],
        pagination: pagination(),
        filters: vec![],
    };

    let content_types = TableSpec {
        name: "content_types".to_string(),
        path: "/content_types".to_string(),
        row_path: RowPath::Pointer {
            pointer: "/items".to_string(),
        },
        columns: vec![
            ColumnSpec {
                name: "id".to_string(),
                field: "sys.id".to_string(),
                data_type: DataType::Text,
            },
            ColumnSpec {
                name: "name".to_string(),
                field: "name".to_string(),
                data_type: DataType::Text,
            },
            ColumnSpec {
                name: "display_field".to_string(),
                field: "displayField".to_string(),
                data_type: DataType::Text,
            },
        ],
        pagination: pagination(),
        filters: vec![],
    };

    SourceSpec {
        name: "contentful".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.access_token.clone(),
        },
        tables: vec![entries, assets, content_types],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use connector_sdk::{Connector, Query};
    use rest_connector::RestConnector;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn spec() -> SourceSpec {
        contentful_spec(&ContentfulConfig {
            base_url: "https://example.com".to_string(),
            access_token: "secret".to_string(),
        })
    }

    #[test]
    fn spec_has_expected_shape() {
        let s = spec();
        assert_eq!(s.name, "contentful");
        assert_eq!(s.base_url, "https://example.com");

        // Auth is Bearer with the configured token.
        assert!(matches!(&s.auth, AuthSpec::Bearer { token } if token == "secret"));

        // Three tables with the expected paths, all using Pointer "/items".
        assert_eq!(s.tables.len(), 3);
        let names: Vec<&str> = s.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["entries", "assets", "content_types"]);

        for (t, expected_path) in s
            .tables
            .iter()
            .zip(["/entries", "/assets", "/content_types"])
        {
            assert_eq!(t.path, expected_path);
            assert!(matches!(&t.row_path, RowPath::Pointer { pointer } if pointer == "/items"));
            assert!(matches!(
                &t.pagination,
                Pagination::Offset { start_param, limit_param, page_size }
                    if start_param == "skip" && limit_param == "limit" && *page_size == 100
            ));
        }

        // entries: four columns and a content_type filter -> param "content_type".
        let entries = &s.tables[0];
        assert_eq!(entries.columns.len(), 4);
        assert_eq!(entries.filters.len(), 1);
        assert_eq!(entries.filters[0].column, "content_type");
        assert_eq!(entries.filters[0].param, "content_type");

        // assets: three columns, no filters.
        let assets = &s.tables[1];
        assert_eq!(assets.columns.len(), 3);
        assert!(assets.filters.is_empty());

        // content_types: three columns, no filters.
        let content_types = &s.tables[2];
        assert_eq!(content_types.columns.len(), 3);
        assert!(content_types.filters.is_empty());
    }

    #[tokio::test]
    async fn fetch_entries_reads_nested_sys_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/entries"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{"sys": {"id": "entry1", "createdAt": "2026-01-01T00:00:00Z"}}]
            })))
            .mount(&server)
            .await;
        let cfg = ContentfulConfig {
            base_url: server.uri(),
            access_token: "t".into(),
        };
        let rows = RestConnector::new(contentful_spec(&cfg))
            .fetch("entries", &Query::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        // the first column is `id` from the nested "sys.id" -> should render as "entry1"
        assert_eq!(rows[0].0[0].to_display_string(), "entry1");
    }
}
