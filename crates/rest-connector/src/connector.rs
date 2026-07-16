//! The config-driven engine: [`RestConnector`] reads a [`SourceSpec`] and
//! implements the `Connector` trait. Fetching handles auth, pagination,
//! equality predicate pushdown (column → query param), and mapping JSON records
//! into neutral rows — all driven by the spec, with no per-source code.

use std::time::Instant;

use async_trait::async_trait;
use connector_sdk::{
    Column, Connector, ConnectorError, DataType, Operator, Query, Result, Row, TableSchema, Value,
};

use crate::spec::{AuthSpec, Pagination, RowPath, SourceSpec, TableSpec};

/// Row cap when the caller didn't specify a `LIMIT`.
const DEFAULT_LIMIT: usize = 100;

/// A connector driven entirely by a [`SourceSpec`].
pub struct RestConnector {
    spec: SourceSpec,
    http: reqwest::Client,
}

impl RestConnector {
    /// Build a connector from a spec. The HTTP client sends a polite default
    /// `User-Agent` (some APIs, e.g. GitHub, require one).
    pub fn new(spec: SourceSpec) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("budbuk-rest-connector/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build HTTP client");
        Self { spec, http }
    }

    /// GET one page and return the parsed JSON body.
    async fn get_body(
        &self,
        table: &TableSpec,
        params: &[(String, String)],
    ) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.spec.base_url, table.path);
        let mut req = self.http.get(&url).query(params);
        req = apply_auth(req, &self.spec.auth);

        let started = Instant::now();
        let resp = req
            .send()
            .await
            .map_err(|e| ConnectorError::Network(e.to_string()))?;
        let status = resp.status();
        let elapsed_ms = started.elapsed().as_millis() as u64;
        tracing::debug!(target: "budbuk::rest", %url, status = status.as_u16(), elapsed_ms);

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 | 403 => ConnectorError::Auth(format!("{status}: {body}")),
                _ => ConnectorError::Other(format!("HTTP {status}: {body}")),
            });
        }
        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| ConnectorError::Parse(e.to_string()))
    }
}

#[async_trait]
impl Connector for RestConnector {
    fn name(&self) -> &str {
        &self.spec.name
    }

    async fn discover(&self) -> Result<Vec<TableSchema>> {
        Ok(self
            .spec
            .tables
            .iter()
            .map(|t| {
                let cols = t
                    .columns
                    .iter()
                    .map(|c| Column {
                        name: c.name.clone(),
                        data_type: c.data_type,
                    })
                    .collect();
                TableSchema::new(&t.name, cols)
            })
            .collect())
    }

    async fn fetch(&self, table_name: &str, query: &Query) -> Result<Vec<Row>> {
        let table = self
            .spec
            .table(table_name)
            .ok_or_else(|| ConnectorError::UnknownTable(table_name.to_string()))?;
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);

        // Predicate pushdown: equality filters on mapped columns become params.
        let base_params = pushdown_params(table, query);

        let mut rows: Vec<Row> = Vec::new();
        match &table.pagination {
            Pagination::None => {
                let body = self.get_body(table, &base_params).await?;
                rows.extend(extract_rows(table, &body)?);
            }
            Pagination::Offset {
                start_param,
                limit_param,
                page_size,
            } => {
                let mut offset = 0usize;
                loop {
                    let want = (limit - rows.len()).min(*page_size);
                    let mut params = base_params.clone();
                    params.push((start_param.clone(), offset.to_string()));
                    params.push((limit_param.clone(), want.to_string()));
                    let batch = extract_rows(table, &self.get_body(table, &params).await?)?;
                    let n = batch.len();
                    rows.extend(batch);
                    offset += n;
                    if rows.len() >= limit || n < want || n == 0 {
                        break;
                    }
                }
            }
            Pagination::Page {
                page_param,
                size_param,
                page_size,
                start_page,
            } => {
                let mut page = *start_page;
                loop {
                    let mut params = base_params.clone();
                    params.push((page_param.clone(), page.to_string()));
                    params.push((size_param.clone(), page_size.to_string()));
                    let batch = extract_rows(table, &self.get_body(table, &params).await?)?;
                    let n = batch.len();
                    rows.extend(batch);
                    page += 1;
                    if rows.len() >= limit || n < *page_size || n == 0 {
                        break;
                    }
                }
            }
            Pagination::Cursor {
                limit_param,
                cursor_param,
                cursor_field,
                more_pointer,
                page_size,
            } => {
                let mut cursor: Option<String> = None;
                loop {
                    let want = (limit - rows.len()).min(*page_size);
                    let mut params = base_params.clone();
                    params.push((limit_param.clone(), want.to_string()));
                    if let Some(c) = &cursor {
                        params.push((cursor_param.clone(), c.clone()));
                    }
                    let body = self.get_body(table, &params).await?;
                    let batch = extract_rows(table, &body)?;
                    let n = batch.len();
                    let next = last_cursor(table, &body, cursor_field);
                    let has_more = body
                        .pointer(more_pointer)
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    rows.extend(batch);
                    match next {
                        Some(c) if has_more && rows.len() < limit && n > 0 => cursor = Some(c),
                        _ => break,
                    }
                }
            }
        }
        rows.truncate(limit);
        Ok(rows)
    }
}

/// Build query params for equality filters that map to a declared param.
fn pushdown_params(table: &TableSpec, query: &Query) -> Vec<(String, String)> {
    let mut params = Vec::new();
    for f in &query.filters {
        if f.op == Operator::Eq {
            if let Some(fp) = table.filters.iter().find(|fp| fp.column == f.column) {
                params.push((fp.param.clone(), f.value.to_display_string()));
            }
        }
    }
    params
}

/// Pull the array of records out of a response and map each to a neutral `Row`.
fn extract_rows(table: &TableSpec, body: &serde_json::Value) -> Result<Vec<Row>> {
    let arr = match &table.row_path {
        RowPath::Root => body.as_array(),
        RowPath::Pointer { pointer } => body.pointer(pointer).and_then(|v| v.as_array()),
    }
    .ok_or_else(|| {
        ConnectorError::Parse(format!(
            "expected an array of records for table '{}'",
            table.name
        ))
    })?;

    Ok(arr
        .iter()
        .map(|item| {
            let cells = table
                .columns
                .iter()
                .map(|c| match resolve_field(item, &c.field) {
                    Some(v) => json_to_value(v, c.data_type),
                    None => Value::Null,
                })
                .collect();
            Row(cells)
        })
        .collect())
}

/// The `cursor_field` value of the last record in a response, as a string —
/// used as the `starting_after` cursor for the next page.
fn last_cursor(table: &TableSpec, body: &serde_json::Value, cursor_field: &str) -> Option<String> {
    let arr = match &table.row_path {
        RowPath::Root => body.as_array(),
        RowPath::Pointer { pointer } => body.pointer(pointer).and_then(|v| v.as_array()),
    }?;
    let last = arr.last()?;
    match resolve_field(last, cursor_field)? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Null => None,
        other => Some(other.to_string()),
    }
}

/// Resolve a dotted field path (e.g. `"user.login"`) within a record.
fn resolve_field<'a>(obj: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut cur = obj;
    for part in path.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur)
}

/// Convert a JSON value to a neutral `Value` for a declared column type.
fn json_to_value(v: &serde_json::Value, dt: DataType) -> Value {
    if v.is_null() {
        return Value::Null;
    }
    match dt {
        DataType::Text => match v {
            serde_json::Value::String(s) => Value::Text(s.clone()),
            other => Value::Text(other.to_string()),
        },
        DataType::Integer => v.as_i64().map(Value::Integer).unwrap_or(Value::Null),
        DataType::Float => v.as_f64().map(Value::Float).unwrap_or(Value::Null),
        DataType::Bool => v.as_bool().map(Value::Bool).unwrap_or(Value::Null),
        DataType::Timestamp => match v.as_str() {
            Some(s) => Value::Timestamp(s.to_string()),
            None => Value::Timestamp(v.to_string()),
        },
        DataType::Json => Value::Json(v.clone()),
    }
}

/// Apply the spec's auth to a request builder.
fn apply_auth(req: reqwest::RequestBuilder, auth: &AuthSpec) -> reqwest::RequestBuilder {
    match auth {
        AuthSpec::None => req,
        AuthSpec::Bearer { token } => req.bearer_auth(token),
        AuthSpec::Basic { username, password } => req.basic_auth(username, Some(password)),
        AuthSpec::ApiKeyHeader { header, value } => req.header(header.as_str(), value),
        AuthSpec::ApiKeyQuery { param, value } => req.query(&[(param.as_str(), value.as_str())]),
        AuthSpec::Headers { headers } => {
            let mut req = req;
            for (k, v) in headers {
                req = req.header(k.as_str(), v);
            }
            req
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use connector_sdk::{Filter, Value as CValue};
    use serde_json::json;

    #[test]
    fn resolve_field_walks_dotted_paths() {
        let obj = json!({"a": {"b": {"c": 1}}, "x": 2});
        assert_eq!(resolve_field(&obj, "x"), Some(&json!(2)));
        assert_eq!(resolve_field(&obj, "a.b.c"), Some(&json!(1)));
        assert!(resolve_field(&obj, "a.b.missing").is_none());
        assert!(resolve_field(&obj, "nope").is_none());
    }

    #[test]
    fn json_to_value_covers_every_type_and_null() {
        assert!(matches!(
            json_to_value(&json!(null), DataType::Text),
            Value::Null
        ));
        assert_eq!(
            json_to_value(&json!("hi"), DataType::Text).to_display_string(),
            "hi"
        );
        // Non-string coerced to text.
        assert_eq!(
            json_to_value(&json!(5), DataType::Text).to_display_string(),
            "5"
        );
        assert!(matches!(
            json_to_value(&json!(7), DataType::Integer),
            Value::Integer(7)
        ));
        // Type mismatch -> Null.
        assert!(matches!(
            json_to_value(&json!("x"), DataType::Integer),
            Value::Null
        ));
        assert!(matches!(
            json_to_value(&json!(1.5), DataType::Float),
            Value::Float(_)
        ));
        assert!(matches!(
            json_to_value(&json!("x"), DataType::Float),
            Value::Null
        ));
        assert!(matches!(
            json_to_value(&json!(true), DataType::Bool),
            Value::Bool(true)
        ));
        assert!(matches!(
            json_to_value(&json!("x"), DataType::Bool),
            Value::Null
        ));
        assert_eq!(
            json_to_value(&json!("2026-01-01"), DataType::Timestamp).to_display_string(),
            "2026-01-01"
        );
        // Non-string timestamp -> stringified.
        assert_eq!(
            json_to_value(&json!(12345), DataType::Timestamp).to_display_string(),
            "12345"
        );
        assert!(matches!(
            json_to_value(&json!({"k": 1}), DataType::Json),
            Value::Json(_)
        ));
    }

    #[test]
    fn pushdown_params_only_maps_declared_equality_filters() {
        let table = TableSpec {
            name: "t".into(),
            path: "/t".into(),
            row_path: RowPath::Root,
            columns: vec![],
            pagination: Pagination::None,
            filters: vec![crate::spec::FilterParam {
                column: "user".into(),
                param: "userId".into(),
            }],
        };
        let query = Query {
            filters: vec![
                Filter::new("user", Operator::Eq, CValue::Integer(1)), // mapped -> pushed
                Filter::new("user", Operator::Gt, CValue::Integer(1)), // not equality -> skip
                Filter::new("other", Operator::Eq, CValue::Text("x".into())), // unmapped -> skip
            ],
            ..Default::default()
        };
        let params = pushdown_params(&table, &query);
        assert_eq!(params, vec![("userId".to_string(), "1".to_string())]);
    }

    fn table_with(row_path: RowPath) -> TableSpec {
        TableSpec {
            name: "t".into(),
            path: "/t".into(),
            row_path,
            columns: vec![],
            pagination: Pagination::None,
            filters: vec![],
        }
    }

    #[test]
    fn last_cursor_reads_last_rows_field_across_shapes() {
        let root = table_with(RowPath::Root);
        // Root array, string id.
        assert_eq!(
            last_cursor(&root, &json!([{"id": "a"}, {"id": "b"}]), "id"),
            Some("b".into())
        );
        // Non-string id is stringified.
        assert_eq!(
            last_cursor(&root, &json!([{"id": 42}]), "id"),
            Some("42".into())
        );
        // Empty array, missing field, and null field all yield None.
        assert_eq!(last_cursor(&root, &json!([]), "id"), None);
        assert_eq!(last_cursor(&root, &json!([{"x": 1}]), "id"), None);
        assert_eq!(last_cursor(&root, &json!([{"id": null}]), "id"), None);
        // Pointer wrapper.
        let ptr = table_with(RowPath::Pointer {
            pointer: "/data".into(),
        });
        assert_eq!(
            last_cursor(&ptr, &json!({"data": [{"id": "z"}]}), "id"),
            Some("z".into())
        );
        // Pointer missing → None.
        assert_eq!(last_cursor(&ptr, &json!({"other": []}), "id"), None);
    }
}
