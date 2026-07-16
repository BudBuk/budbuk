//! The config-driven engine: [`GraphQlConnector`] reads a [`GraphQlSpec`] and
//! implements the `Connector` trait. Fetching handles auth, GraphQL errors,
//! Relay cursor pagination, equality predicate pushdown (column → GraphQL
//! variable), and mapping JSON nodes into neutral rows — all driven by the spec,
//! with no per-source code.

use std::time::Instant;

use async_trait::async_trait;
use connector_sdk::{
    Column, Connector, ConnectorError, DataType, Operator, Query, Result, Row, TableSchema, Value,
};
use serde_json::json;

use crate::spec::{AuthSpec, GraphQlSpec, GraphQlTable, NodeShape, Pagination};

/// Row cap when the caller didn't specify a `LIMIT`.
const DEFAULT_LIMIT: usize = 100;

/// A connector driven entirely by a [`GraphQlSpec`].
pub struct GraphQlConnector {
    spec: GraphQlSpec,
    http: reqwest::Client,
}

impl GraphQlConnector {
    /// Build a connector from a spec.
    pub fn new(spec: GraphQlSpec) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                "budbuk-graphql-connector/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("failed to build HTTP client");
        Self { spec, http }
    }

    /// POST one GraphQL request (`{query, variables}`) and return the parsed
    /// response body. GraphQL reports query errors in an `errors` array even on
    /// HTTP 200, so those are surfaced here too.
    async fn post(
        &self,
        query: &str,
        variables: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let payload = json!({ "query": query, "variables": variables });
        let mut req = self.http.post(&self.spec.endpoint).json(&payload);
        req = apply_auth(req, &self.spec.auth);

        let started = Instant::now();
        let resp = req
            .send()
            .await
            .map_err(|e| ConnectorError::Network(e.to_string()))?;
        let status = resp.status();
        let elapsed_ms = started.elapsed().as_millis() as u64;
        tracing::debug!(target: "budbuk::graphql", endpoint = %self.spec.endpoint, status = status.as_u16(), elapsed_ms);

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 | 403 => ConnectorError::Auth(format!("{status}: {body}")),
                _ => ConnectorError::Other(format!("HTTP {status}: {body}")),
            });
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ConnectorError::Parse(e.to_string()))?;

        if let Some(errors) = body.get("errors").and_then(|e| e.as_array()) {
            if !errors.is_empty() {
                let msg = errors
                    .iter()
                    .map(|e| {
                        e.get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown error")
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(ConnectorError::Other(format!("GraphQL error: {msg}")));
            }
        }
        Ok(body)
    }
}

#[async_trait]
impl Connector for GraphQlConnector {
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

        // Predicate pushdown: equality filters on mapped columns become variables.
        let base_vars = pushdown_vars(table, query);

        let mut rows: Vec<Row> = Vec::new();
        match &table.pagination {
            Pagination::None => {
                let body = self.post(&table.query, &base_vars).await?;
                rows.extend(extract_rows(table, &body)?);
            }
            Pagination::Relay {
                first_var,
                after_var,
                page_size,
            } => {
                let mut after: Option<String> = None;
                loop {
                    let want = (limit - rows.len()).min(*page_size);
                    let mut vars = base_vars.clone();
                    vars.insert(first_var.clone(), json!(want));
                    if let Some(a) = &after {
                        vars.insert(after_var.clone(), json!(a));
                    }
                    let body = self.post(&table.query, &vars).await?;
                    let batch = extract_rows(table, &body)?;
                    let n = batch.len();
                    rows.extend(batch);
                    let (has_next, end_cursor) = page_info(table, &body);
                    match end_cursor {
                        Some(c) if has_next && rows.len() < limit && n > 0 => after = Some(c),
                        _ => break,
                    }
                }
            }
        }
        rows.truncate(limit);
        Ok(rows)
    }
}

/// Build the GraphQL variables for equality filters that map to a declared variable.
fn pushdown_vars(
    table: &GraphQlTable,
    query: &Query,
) -> serde_json::Map<String, serde_json::Value> {
    let mut vars = serde_json::Map::new();
    for f in &query.filters {
        if f.op == Operator::Eq {
            if let Some(fv) = table.filters.iter().find(|fv| fv.column == f.column) {
                vars.insert(fv.variable.clone(), cvalue_to_json(&f.value));
            }
        }
    }
    vars
}

/// Collect the node objects addressed by a table's `data_pointer` (relative to
/// the response's `data`), unwrapping a Relay connection's `edges/node` when the
/// table's shape says so.
fn node_values<'a>(
    table: &GraphQlTable,
    body: &'a serde_json::Value,
) -> Result<Vec<&'a serde_json::Value>> {
    let full = format!("/data{}", table.data_pointer);
    let target = body.pointer(&full).ok_or_else(|| {
        ConnectorError::Parse(format!("no data at '{}' for table '{}'", full, table.name))
    })?;
    match table.shape {
        NodeShape::List => target
            .as_array()
            .map(|a| a.iter().collect())
            .ok_or_else(|| {
                ConnectorError::Parse(format!(
                    "expected a list at '{}' for table '{}'",
                    full, table.name
                ))
            }),
        NodeShape::Connection => {
            let edges = target
                .pointer("/edges")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    ConnectorError::Parse(format!(
                        "expected connection edges at '{}' for table '{}'",
                        full, table.name
                    ))
                })?;
            Ok(edges.iter().filter_map(|e| e.pointer("/node")).collect())
        }
    }
}

/// Map each addressed node to a neutral `Row` using the table's columns.
fn extract_rows(table: &GraphQlTable, body: &serde_json::Value) -> Result<Vec<Row>> {
    let nodes = node_values(table, body)?;
    Ok(nodes
        .iter()
        .map(|node| {
            let cells = table
                .columns
                .iter()
                .map(|c| match resolve_field(node, &c.field) {
                    Some(v) => json_to_value(v, c.data_type),
                    None => Value::Null,
                })
                .collect();
            Row(cells)
        })
        .collect())
}

/// Read a Relay connection's `pageInfo.hasNextPage` and `pageInfo.endCursor`.
fn page_info(table: &GraphQlTable, body: &serde_json::Value) -> (bool, Option<String>) {
    let base = format!("/data{}/pageInfo", table.data_pointer);
    let has_next = body
        .pointer(&format!("{base}/hasNextPage"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let end_cursor = body
        .pointer(&format!("{base}/endCursor"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    (has_next, end_cursor)
}

/// Convert a neutral filter value into a JSON variable value.
fn cvalue_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Text(s) => json!(s),
        Value::Integer(n) => json!(n),
        Value::Float(f) => json!(f),
        Value::Bool(b) => json!(b),
        Value::Timestamp(s) => json!(s),
        Value::Json(j) => j.clone(),
    }
}

/// Resolve a dotted field path (e.g. `"continent.code"`) within a node.
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{ColumnSpec, FilterVar, GraphQlTable};
    use connector_sdk::{Filter, Value as CValue};
    use wiremock::matchers::{body_string_contains, header, header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn list_table() -> GraphQlTable {
        GraphQlTable {
            name: "countries".into(),
            query: "query($code:ID){ countries { code name } }".into(),
            data_pointer: "/countries".into(),
            shape: NodeShape::List,
            columns: vec![
                ColumnSpec {
                    name: "code".into(),
                    field: "code".into(),
                    data_type: DataType::Text,
                },
                ColumnSpec {
                    name: "name".into(),
                    field: "name".into(),
                    data_type: DataType::Text,
                },
            ],
            pagination: Pagination::None,
            filters: vec![FilterVar {
                column: "code".into(),
                variable: "code".into(),
            }],
        }
    }

    fn conn_table() -> GraphQlTable {
        GraphQlTable {
            name: "issues".into(),
            query: "query($first:Int,$after:String){ issues(first:$first,after:$after){ edges{ node{ id } } pageInfo{ hasNextPage endCursor } } }".into(),
            data_pointer: "/issues".into(),
            shape: NodeShape::Connection,
            columns: vec![ColumnSpec {
                name: "id".into(),
                field: "id".into(),
                data_type: DataType::Text,
            }],
            pagination: Pagination::Relay {
                first_var: "first".into(),
                after_var: "after".into(),
                page_size: 2,
            },
            filters: vec![],
        }
    }

    fn spec_with(table: GraphQlTable, endpoint: String, auth: AuthSpec) -> GraphQlSpec {
        GraphQlSpec {
            name: "t".into(),
            endpoint,
            auth,
            tables: vec![table],
        }
    }

    #[tokio::test]
    async fn fetches_a_plain_list() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"countries": [
                    {"code": "US", "name": "United States"},
                    {"code": "IN", "name": "India"}
                ]}
            })))
            .mount(&server)
            .await;

        let conn = GraphQlConnector::new(spec_with(
            list_table(),
            format!("{}/graphql", server.uri()),
            AuthSpec::None,
        ));
        let rows = conn.fetch("countries", &Query::default()).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0[0].to_display_string(), "US");
        assert_eq!(rows[1].0[1].to_display_string(), "India");
    }

    #[tokio::test]
    async fn pushes_equality_filter_to_a_variable_and_applies_auth() {
        let server = MockServer::start().await;
        // Only responds when the bearer token AND the pushed variable are present.
        Mock::given(method("POST"))
            .and(header("authorization", "Bearer s3cr3t"))
            .and(body_string_contains("\"code\":\"US\""))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"countries": [{"code": "US", "name": "United States"}]}
            })))
            .mount(&server)
            .await;

        let conn = GraphQlConnector::new(spec_with(
            list_table(),
            format!("{}/graphql", server.uri()),
            AuthSpec::Bearer {
                token: "s3cr3t".into(),
            },
        ));
        let query = Query {
            filters: vec![
                Filter::new("code", Operator::Eq, CValue::Text("US".into())),
                // Non-equality and unmapped filters are ignored.
                Filter::new("code", Operator::Gt, CValue::Text("A".into())),
                Filter::new("name", Operator::Eq, CValue::Text("x".into())),
            ],
            ..Default::default()
        };
        let rows = conn.fetch("countries", &query).await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn follows_relay_cursor_pagination() {
        let server = MockServer::start().await;
        // Page 2: request body carries the cursor from page 1.
        Mock::given(method("POST"))
            .and(body_string_contains("CURSOR1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"issues": {
                    "edges": [{"node": {"id": "3"}}],
                    "pageInfo": {"hasNextPage": false, "endCursor": "CURSOR2"}
                }}
            })))
            .mount(&server)
            .await;
        // Page 1: no cursor yet. hasNextPage=true, so the engine pages again.
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"issues": {
                    "edges": [{"node": {"id": "1"}}, {"node": {"id": "2"}}],
                    "pageInfo": {"hasNextPage": true, "endCursor": "CURSOR1"}
                }}
            })))
            .mount(&server)
            .await;

        let conn = GraphQlConnector::new(spec_with(
            conn_table(),
            format!("{}/graphql", server.uri()),
            AuthSpec::None,
        ));
        let rows = conn.fetch("issues", &Query::default()).await.unwrap();
        let ids: Vec<String> = rows.iter().map(|r| r.0[0].to_display_string()).collect();
        assert_eq!(ids, vec!["1", "2", "3"]);
    }

    #[tokio::test]
    async fn connection_skips_edges_without_a_node_and_stops_without_cursor() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"issues": {
                    "edges": [{"node": {"id": "1"}}, {"cursor": "x"}],
                    "pageInfo": {"hasNextPage": true, "endCursor": null}
                }}
            })))
            .mount(&server)
            .await;

        let conn = GraphQlConnector::new(spec_with(
            conn_table(),
            format!("{}/graphql", server.uri()),
            AuthSpec::None,
        ));
        // endCursor is null → loop stops after one page; the node-less edge is skipped.
        let rows = conn.fetch("issues", &Query::default()).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0[0].to_display_string(), "1");
    }

    #[tokio::test]
    async fn honors_limit_across_pages() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"issues": {
                    "edges": [{"node": {"id": "1"}}, {"node": {"id": "2"}}],
                    "pageInfo": {"hasNextPage": true, "endCursor": "C"}
                }}
            })))
            .mount(&server)
            .await;

        let conn = GraphQlConnector::new(spec_with(
            conn_table(),
            format!("{}/graphql", server.uri()),
            AuthSpec::None,
        ));
        let query = Query {
            limit: Some(1),
            ..Default::default()
        };
        let rows = conn.fetch("issues", &query).await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn surfaces_graphql_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "errors": [{"message": "Field 'x' doesn't exist"}, {"other": true}]
            })))
            .mount(&server)
            .await;

        let conn = GraphQlConnector::new(spec_with(
            list_table(),
            format!("{}/graphql", server.uri()),
            AuthSpec::None,
        ));
        let err = conn
            .fetch("countries", &Query::default())
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("GraphQL error"));
        assert!(msg.contains("Field 'x'"));
        assert!(msg.contains("unknown error"));
    }

    #[tokio::test]
    async fn maps_http_auth_and_other_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_string_contains("unauth"))
            .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        // 401 → Auth
        let auth_conn = GraphQlConnector::new(spec_with(
            GraphQlTable {
                query: "query{ unauth }".into(),
                ..list_table()
            },
            format!("{}/graphql", server.uri()),
            AuthSpec::None,
        ));
        assert!(matches!(
            auth_conn.fetch("countries", &Query::default()).await,
            Err(ConnectorError::Auth(_))
        ));

        // 500 → Other
        let other_conn = GraphQlConnector::new(spec_with(
            list_table(),
            format!("{}/graphql", server.uri()),
            AuthSpec::None,
        ));
        assert!(matches!(
            other_conn.fetch("countries", &Query::default()).await,
            Err(ConnectorError::Other(_))
        ));
    }

    #[tokio::test]
    async fn network_failure_becomes_network_error() {
        // Port 1 is (practically) always closed → immediate connection refused.
        let conn = GraphQlConnector::new(spec_with(
            list_table(),
            "http://127.0.0.1:1/graphql".into(),
            AuthSpec::None,
        ));
        assert!(matches!(
            conn.fetch("countries", &Query::default()).await,
            Err(ConnectorError::Network(_))
        ));
    }

    #[tokio::test]
    async fn parse_error_when_body_is_not_json() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        let conn = GraphQlConnector::new(spec_with(
            list_table(),
            format!("{}/graphql", server.uri()),
            AuthSpec::None,
        ));
        assert!(matches!(
            conn.fetch("countries", &Query::default()).await,
            Err(ConnectorError::Parse(_))
        ));
    }

    #[tokio::test]
    async fn shape_mismatches_are_parse_errors() {
        let server = MockServer::start().await;
        // data present but the pointer target is missing / wrong shape.
        Mock::given(method("POST"))
            .and(body_string_contains("missing"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {}})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(body_string_contains("notarray"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"data": {"countries": 5}})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"issues": {}}})))
            .mount(&server)
            .await;
        let uri = format!("{}/graphql", server.uri());

        // Missing pointer target.
        let missing = GraphQlConnector::new(spec_with(
            GraphQlTable {
                query: "query{ missing }".into(),
                ..list_table()
            },
            uri.clone(),
            AuthSpec::None,
        ));
        assert!(matches!(
            missing.fetch("countries", &Query::default()).await,
            Err(ConnectorError::Parse(_))
        ));

        // List shape but not an array.
        let notarray = GraphQlConnector::new(spec_with(
            GraphQlTable {
                query: "query{ notarray }".into(),
                ..list_table()
            },
            uri.clone(),
            AuthSpec::None,
        ));
        assert!(matches!(
            notarray.fetch("countries", &Query::default()).await,
            Err(ConnectorError::Parse(_))
        ));

        // Connection shape but no edges.
        let noedges = GraphQlConnector::new(spec_with(conn_table(), uri, AuthSpec::None));
        assert!(matches!(
            noedges.fetch("issues", &Query::default()).await,
            Err(ConnectorError::Parse(_))
        ));
    }

    #[tokio::test]
    async fn unknown_table_errors_and_discover_lists_columns() {
        let conn = GraphQlConnector::new(spec_with(
            list_table(),
            "http://127.0.0.1:1/graphql".into(),
            AuthSpec::None,
        ));
        assert_eq!(conn.name(), "t");
        assert!(matches!(
            conn.fetch("nope", &Query::default()).await,
            Err(ConnectorError::UnknownTable(_))
        ));
        let schemas = conn.discover().await.unwrap();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "countries");
        assert_eq!(schemas[0].columns.len(), 2);
    }

    #[tokio::test]
    async fn empty_errors_array_is_ignored() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"countries": [{"code": "US", "name": "United States"}]},
                "errors": []
            })))
            .mount(&server)
            .await;
        let conn = GraphQlConnector::new(spec_with(
            list_table(),
            format!("{}/graphql", server.uri()),
            AuthSpec::None,
        ));
        let rows = conn.fetch("countries", &Query::default()).await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn missing_node_field_becomes_null() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"countries": [{"code": "US"}]} // no "name"
            })))
            .mount(&server)
            .await;
        let conn = GraphQlConnector::new(spec_with(
            list_table(),
            format!("{}/graphql", server.uri()),
            AuthSpec::None,
        ));
        let rows = conn.fetch("countries", &Query::default()).await.unwrap();
        assert_eq!(rows[0].0[0].to_display_string(), "US");
        assert!(matches!(rows[0].0[1], Value::Null));
    }

    #[tokio::test]
    async fn basic_and_api_key_header_auth_are_applied() {
        let server = MockServer::start().await;
        // Basic auth path: require an Authorization header (reqwest sets `Basic <b64>`).
        Mock::given(method("POST"))
            .and(header_exists("authorization"))
            .and(body_string_contains("basic"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"countries": [{"code": "US", "name": "US"}]}
            })))
            .mount(&server)
            .await;
        // API-key header path: require the fixed header the spec declares.
        Mock::given(method("POST"))
            .and(header("X-API-Key", "k3y"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"countries": [{"code": "IN", "name": "IN"}]}
            })))
            .mount(&server)
            .await;
        let uri = format!("{}/graphql", server.uri());

        let basic = GraphQlConnector::new(spec_with(
            GraphQlTable {
                query: "query{ basic }".into(),
                ..list_table()
            },
            uri.clone(),
            AuthSpec::Basic {
                username: "u".into(),
                password: "p".into(),
            },
        ));
        assert_eq!(
            basic
                .fetch("countries", &Query::default())
                .await
                .unwrap()
                .len(),
            1
        );

        let api_key = GraphQlConnector::new(spec_with(
            list_table(),
            uri,
            AuthSpec::ApiKeyHeader {
                header: "X-API-Key".into(),
                value: "k3y".into(),
            },
        ));
        assert_eq!(
            api_key
                .fetch("countries", &Query::default())
                .await
                .unwrap()
                .len(),
            1
        );
    }

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
        assert_eq!(
            json_to_value(&json!(5), DataType::Text).to_display_string(),
            "5"
        );
        assert!(matches!(
            json_to_value(&json!(7), DataType::Integer),
            Value::Integer(7)
        ));
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
    fn cvalue_to_json_covers_every_variant() {
        assert_eq!(cvalue_to_json(&CValue::Null), json!(null));
        assert_eq!(cvalue_to_json(&CValue::Text("s".into())), json!("s"));
        assert_eq!(cvalue_to_json(&CValue::Integer(3)), json!(3));
        assert_eq!(cvalue_to_json(&CValue::Float(1.5)), json!(1.5));
        assert_eq!(cvalue_to_json(&CValue::Bool(true)), json!(true));
        assert_eq!(
            cvalue_to_json(&CValue::Timestamp("2026".into())),
            json!("2026")
        );
        assert_eq!(cvalue_to_json(&CValue::Json(json!([1]))), json!([1]));
    }
}
