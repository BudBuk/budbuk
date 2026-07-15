//! The declarative description of a REST source.
//!
//! A [`SourceSpec`] fully describes an API: its base URL, how to authenticate,
//! and a list of [`TableSpec`]s (endpoints exposed as tables). The
//! [`RestConnector`](crate::RestConnector) reads *any* spec and implements the
//! `Connector` trait — so a hand-written spec and an OpenAPI-generated spec run
//! through the exact same engine. Everything derives `serde`, so specs can be
//! loaded from JSON/TOML or produced by an OpenAPI importer.

use connector_sdk::DataType;
use serde::{Deserialize, Serialize};

/// A complete description of one REST API source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpec {
    /// Short identifier for this source, e.g. `"github"`.
    pub name: String,
    /// Base URL, e.g. `"https://api.github.com"`. Table paths are appended.
    pub base_url: String,
    /// How to authenticate requests.
    #[serde(default)]
    pub auth: AuthSpec,
    /// The tables this source exposes.
    pub tables: Vec<TableSpec>,
}

impl SourceSpec {
    /// Look up a table spec by name.
    pub fn table(&self, name: &str) -> Option<&TableSpec> {
        self.tables.iter().find(|t| t.name == name)
    }
}

/// How to authenticate requests to the source.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthSpec {
    /// No authentication (public API).
    #[default]
    None,
    /// `Authorization: Bearer <token>`.
    Bearer { token: String },
    /// HTTP Basic auth.
    Basic { username: String, password: String },
    /// A fixed header, e.g. `X-API-Key: <value>`.
    ApiKeyHeader { header: String, value: String },
    /// A fixed query parameter, e.g. `?api_key=<value>`.
    ApiKeyQuery { param: String, value: String },
}

/// One table: an endpoint that returns a list of records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSpec {
    /// The table name, e.g. `"issues"`.
    pub name: String,
    /// Path appended to the source's `base_url`, e.g. `"/repos/o/r/issues"`.
    pub path: String,
    /// Where the array of records lives in the JSON response.
    #[serde(default)]
    pub row_path: RowPath,
    /// The columns to project from each record.
    pub columns: Vec<ColumnSpec>,
    /// How the endpoint paginates.
    #[serde(default)]
    pub pagination: Pagination,
    /// Column → query-param mappings enabling equality predicate pushdown.
    #[serde(default)]
    pub filters: Vec<FilterParam>,
}

/// One column: its name, where to read it from a record, and its type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSpec {
    /// The SQL column name.
    pub name: String,
    /// Dotted path into a record object, e.g. `"title"` or `"user.login"`.
    pub field: String,
    /// The column's type.
    pub data_type: DataType,
}

/// Where the array of records is located in the response body.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RowPath {
    /// The response body itself is the array.
    #[default]
    Root,
    /// The array is at a JSON pointer, e.g. `"/data"` or `"/items"`.
    Pointer { pointer: String },
}

/// How an endpoint paginates.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "style", rename_all = "snake_case")]
pub enum Pagination {
    /// Single request, no pagination.
    #[default]
    None,
    /// Offset/limit style, e.g. `?_start=0&_limit=20`.
    Offset {
        start_param: String,
        limit_param: String,
        page_size: usize,
    },
    /// Page-number style, e.g. `?page=1&per_page=20`.
    Page {
        page_param: String,
        size_param: String,
        page_size: usize,
        start_page: usize,
    },
}

/// Maps a column to the query parameter used to filter it (equality pushdown).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterParam {
    /// The column name as declared in [`ColumnSpec`].
    pub column: String,
    /// The query parameter to send when this column is filtered with `=`.
    pub param: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_spec_round_trips_through_json() {
        let spec = SourceSpec {
            name: "demo".into(),
            base_url: "https://example.com".into(),
            auth: AuthSpec::Bearer { token: "t".into() },
            tables: vec![TableSpec {
                name: "posts".into(),
                path: "/posts".into(),
                row_path: RowPath::Root,
                columns: vec![ColumnSpec {
                    name: "id".into(),
                    field: "id".into(),
                    data_type: DataType::Integer,
                }],
                pagination: Pagination::Offset {
                    start_param: "_start".into(),
                    limit_param: "_limit".into(),
                    page_size: 20,
                },
                filters: vec![FilterParam {
                    column: "user".into(),
                    param: "userId".into(),
                }],
            }],
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: SourceSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "demo");
        assert_eq!(back.table("posts").unwrap().columns.len(), 1);
        assert!(back.table("missing").is_none());
        // Exercise Debug/Clone on the surviving value.
        assert!(!format!("{back:?}").is_empty());
        let _ = back.clone();
    }

    #[test]
    fn defaults_deserialize_when_omitted() {
        // auth, row_path, pagination, filters all default.
        let json = r#"{
            "name": "d", "base_url": "https://x",
            "tables": [{"name":"t","path":"/t","columns":[{"name":"a","field":"a","data_type":"Text"}]}]
        }"#;
        let spec: SourceSpec = serde_json::from_str(json).unwrap();
        assert!(matches!(spec.auth, AuthSpec::None));
        let t = &spec.tables[0];
        assert!(matches!(t.row_path, RowPath::Root));
        assert!(matches!(t.pagination, Pagination::None));
        assert!(t.filters.is_empty());
    }

    #[test]
    fn every_auth_and_pagination_variant_serializes() {
        for auth in [
            AuthSpec::None,
            AuthSpec::Bearer { token: "t".into() },
            AuthSpec::Basic {
                username: "u".into(),
                password: "p".into(),
            },
            AuthSpec::ApiKeyHeader {
                header: "H".into(),
                value: "v".into(),
            },
            AuthSpec::ApiKeyQuery {
                param: "k".into(),
                value: "v".into(),
            },
        ] {
            let s = serde_json::to_string(&auth).unwrap();
            let _: AuthSpec = serde_json::from_str(&s).unwrap();
            assert!(!format!("{auth:?}").is_empty());
        }
        for pg in [
            Pagination::None,
            Pagination::Offset {
                start_param: "s".into(),
                limit_param: "l".into(),
                page_size: 10,
            },
            Pagination::Page {
                page_param: "p".into(),
                size_param: "n".into(),
                page_size: 10,
                start_page: 1,
            },
        ] {
            let s = serde_json::to_string(&pg).unwrap();
            let _: Pagination = serde_json::from_str(&s).unwrap();
            let _ = pg.clone();
        }
        // RowPath::Pointer + FilterParam Debug/Clone.
        let rp = RowPath::Pointer {
            pointer: "/data".into(),
        };
        let _ = rp.clone();
        assert!(!format!("{rp:?}").is_empty());
        let fp = FilterParam {
            column: "c".into(),
            param: "p".into(),
        };
        assert!(!format!("{fp:?}").is_empty());
    }
}
