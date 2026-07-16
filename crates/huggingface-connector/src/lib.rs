//! BudBuk connector for the Hugging Face Hub.
//!
//! Exposes the public Hub listing endpoints (`/api/models`, `/api/datasets`,
//! `/api/spaces`) as a [`SourceSpec`] over the shared REST engine. Uses a bearer
//! access token; the endpoints return a JSON array (default 1000 rows) so no
//! pagination is configured. Equality filters on `author`/`search` push down.

use connector_sdk::DataType;
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec,
};

/// One Hugging Face target.
pub struct HuggingFaceConfig {
    /// Base URL, defaults to `https://huggingface.co`.
    pub base_url: String,
    /// A user access token (`hf_…`).
    pub token: String,
}

fn col(name: &str, field: &str, data_type: DataType) -> ColumnSpec {
    ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    }
}

/// Build a [`SourceSpec`] for the Hugging Face Hub.
pub fn huggingface_spec(cfg: &HuggingFaceConfig) -> SourceSpec {
    let author = || FilterParam {
        column: "author".to_string(),
        param: "author".to_string(),
    };
    SourceSpec {
        name: "huggingface".to_string(),
        base_url: cfg.base_url.clone(),
        auth: AuthSpec::Bearer {
            token: cfg.token.clone(),
        },
        tables: vec![
            TableSpec {
                name: "models".to_string(),
                path: "/api/models".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("likes", "likes", DataType::Integer),
                    col("downloads", "downloads", DataType::Integer),
                    col("private", "private", DataType::Bool),
                ],
                pagination: Pagination::None,
                filters: vec![
                    author(),
                    FilterParam {
                        column: "search".to_string(),
                        param: "search".to_string(),
                    },
                ],
            },
            TableSpec {
                name: "datasets".to_string(),
                path: "/api/datasets".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("author", "author", DataType::Text),
                    col("likes", "likes", DataType::Integer),
                ],
                pagination: Pagination::None,
                filters: vec![author()],
            },
            TableSpec {
                name: "spaces".to_string(),
                path: "/api/spaces".to_string(),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("likes", "likes", DataType::Integer),
                    col("sdk", "sdk", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![author()],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use connector_sdk::{Connector, Filter, Operator, Query, Value};
    use rest_connector::RestConnector;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn spec_exposes_expected_tables_and_bearer_auth() {
        let spec = huggingface_spec(&HuggingFaceConfig {
            base_url: "https://huggingface.co".to_string(),
            token: "hf_x".to_string(),
        });
        assert_eq!(spec.name, "huggingface");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        assert!(spec.table("models").is_some());
        assert!(spec.table("datasets").is_some());
        assert_eq!(spec.table("spaces").unwrap().columns.len(), 3);
        assert!(spec.table("missing").is_none());
    }

    #[tokio::test]
    async fn fetches_models_and_pushes_down_search() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/models"))
            .and(query_param("search", "bert"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "google-bert/bert-base", "likes": 100, "downloads": 5000, "private": false}
            ])))
            .mount(&server)
            .await;

        let spec = huggingface_spec(&HuggingFaceConfig {
            base_url: server.uri(),
            token: "hf_x".to_string(),
        });
        let query = Query {
            filters: vec![Filter::new(
                "search",
                Operator::Eq,
                Value::Text("bert".into()),
            )],
            ..Default::default()
        };
        let rows = RestConnector::new(spec)
            .fetch("models", &query)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0[0].to_display_string(), "google-bert/bert-base");
    }
}
