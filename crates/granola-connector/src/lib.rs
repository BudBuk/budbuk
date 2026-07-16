//! BudBuk connector for Granola (AI meeting notes).
//!
//! Wraps Granola's public API (`https://public-api.granola.ai/v1`), which uses a
//! bearer `grn_` token. Currently exposes the `/notes` collection. Granola
//! paginates with a top-level cursor the shared engine can't follow, so this
//! fetches a single page for now.
//!
//! NOTE: the response envelope (row path `/data`) is per Granola's documented
//! shape; it should be confirmed against a live 200 response with a valid key.

use connector_sdk::DataType;
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec,
};

/// One Granola target.
pub struct GranolaConfig {
    /// Base URL, defaults to `https://public-api.granola.ai/v1`.
    pub base_url: String,
    /// A Granola API key (`grn_…`).
    pub api_key: String,
}

fn col(name: &str, field: &str, data_type: DataType) -> ColumnSpec {
    ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    }
}

/// Build a [`SourceSpec`] for Granola.
pub fn granola_spec(cfg: &GranolaConfig) -> SourceSpec {
    SourceSpec {
        name: "granola".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.api_key.clone(),
        },
        tables: vec![TableSpec {
            name: "notes".to_string(),
            path: "/notes".to_string(),
            row_path: RowPath::Pointer {
                pointer: "/data".to_string(),
            },
            columns: vec![
                col("id", "id", DataType::Text),
                col("title", "title", DataType::Text),
                col("owner_name", "owner.name", DataType::Text),
                col("owner_email", "owner.email", DataType::Text),
                col("summary", "summary", DataType::Text),
            ],
            pagination: Pagination::None,
            filters: vec![FilterParam {
                column: "created_after".to_string(),
                param: "created_after".to_string(),
            }],
        }],
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
    fn spec_exposes_notes_and_bearer_auth() {
        let spec = granola_spec(&GranolaConfig {
            base_url: "https://public-api.granola.ai/v1".to_string(),
            api_key: "grn_x".to_string(),
        });
        assert_eq!(spec.name, "granola");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        let notes = spec.table("notes").unwrap();
        assert_eq!(notes.columns.len(), 5);
        assert!(spec.table("missing").is_none());
    }

    #[tokio::test]
    async fn fetches_notes_with_nested_owner() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/notes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": "not_1", "title": "Standup", "owner": {"name": "Ada", "email": "a@b.c"}, "summary": "notes"}
                ],
                "hasMore": false
            })))
            .mount(&server)
            .await;
        let rows = RestConnector::new(granola_spec(&GranolaConfig {
            base_url: server.uri(),
            api_key: "grn_x".to_string(),
        }))
        .fetch("notes", &Query::default())
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0[0].to_display_string(), "not_1");
        assert_eq!(rows[0].0[2].to_display_string(), "Ada"); // nested owner.name
    }
}
