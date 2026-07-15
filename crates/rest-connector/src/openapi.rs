//! Generate a [`SourceSpec`] from an OpenAPI 3 document — the "force
//! multiplier". Point the engine at any OpenAPI spec and get a working
//! connector with no hand-written config.
//!
//! Heuristics (pragmatic, not a full OpenAPI implementation):
//! - Only `GET` operations on **collection paths** (no `{path params}`) become
//!   tables — those are the list endpoints.
//! - A table's rows come from an array response (`RowPath::Root`) or from a
//!   single array property of a wrapper object (`RowPath::Pointer`, preferring
//!   `data`/`items`/`results`/`records`/`values`).
//! - Columns come from the item object's `properties`; OpenAPI types map to
//!   [`DataType`]. Local `$ref`s (`#/components/...`) are resolved.
//! - Query parameters whose name matches a column become equality-pushdown
//!   filters.

use connector_sdk::DataType;
use serde_json::Value as Json;

use crate::spec::{AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec};

/// Something went wrong turning an OpenAPI document into a spec.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The document was malformed or missing required pieces.
    #[error("invalid OpenAPI document: {0}")]
    Invalid(String),
    /// The document parsed but exposed no queryable collection endpoints.
    #[error("no queryable collection endpoints found in the OpenAPI document")]
    NoTables,
}

/// Knobs for the import. The document has no credentials, so `auth` is supplied
/// here; `base_url` overrides `servers[0].url`; `name` overrides `info.title`.
#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub auth: AuthSpec,
    /// If set, only tables whose name is in this list are kept (a big spec like
    /// Stripe's exposes ~100 tables; use this to focus on a few).
    pub include: Option<Vec<String>>,
}

impl SourceSpec {
    /// Build a spec from an already-parsed OpenAPI document.
    pub fn from_openapi(doc: &Json, opts: ImportOptions) -> Result<SourceSpec, ImportError> {
        import(doc, opts)
    }

    /// Build a spec from an OpenAPI document as a JSON string.
    pub fn from_openapi_json(json: &str, opts: ImportOptions) -> Result<SourceSpec, ImportError> {
        let doc: Json =
            serde_json::from_str(json).map_err(|e| ImportError::Invalid(e.to_string()))?;
        Self::from_openapi(&doc, opts)
    }
}

fn import(doc: &Json, opts: ImportOptions) -> Result<SourceSpec, ImportError> {
    let base_url = opts
        .base_url
        .or_else(|| {
            doc.pointer("/servers/0/url")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .ok_or_else(|| {
            ImportError::Invalid("missing servers[0].url and no base_url override".into())
        })?;

    let name = opts
        .name
        .or_else(|| {
            doc.pointer("/info/title")
                .and_then(Json::as_str)
                .map(sanitize)
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "openapi".to_string());

    let paths = doc
        .get("paths")
        .and_then(Json::as_object)
        .ok_or_else(|| ImportError::Invalid("missing 'paths'".into()))?;

    let include = opts.include.clone();
    let mut tables: Vec<TableSpec> = Vec::new();
    let mut used_names: Vec<String> = Vec::new();

    for (path, item) in paths {
        if path.contains('{') {
            continue; // single-item endpoint, not a collection
        }
        let Some(op) = item.get("get") else { continue };
        let Some(schema) = success_schema(doc, op) else {
            continue;
        };
        let Some((row_path, item_schema)) = array_item_schema(doc, &schema) else {
            continue;
        };
        let Some(props) = item_schema.get("properties").and_then(Json::as_object) else {
            continue;
        };
        if props.is_empty() {
            continue;
        }

        let columns: Vec<ColumnSpec> = props
            .iter()
            .map(|(k, sch)| ColumnSpec {
                name: k.clone(),
                field: k.clone(),
                data_type: schema_datatype(doc, sch),
            })
            .collect();

        let qparams = query_params(op);
        let filters: Vec<FilterParam> = qparams
            .iter()
            .filter(|p| props.contains_key(p.as_str()))
            .map(|p| FilterParam {
                column: p.clone(),
                param: p.clone(),
            })
            .collect();

        // Detect cursor pagination (Stripe-style: `starting_after` + `limit`).
        let pagination = if qparams.iter().any(|p| p == "starting_after") {
            Pagination::Cursor {
                limit_param: "limit".to_string(),
                cursor_param: "starting_after".to_string(),
                cursor_field: "id".to_string(),
                more_pointer: "/has_more".to_string(),
                page_size: 100,
            }
        } else {
            Pagination::None
        };

        let raw = last_segment(path)
            .or_else(|| op.get("operationId").and_then(Json::as_str).map(sanitize))
            .unwrap_or_default();
        if let Some(inc) = &include {
            if !inc.iter().any(|n| n == &raw) {
                continue;
            }
        }
        let table_name = dedup(raw, &mut used_names);

        tables.push(TableSpec {
            name: table_name,
            path: path.clone(),
            row_path,
            columns,
            pagination,
            filters,
        });
    }

    if tables.is_empty() {
        return Err(ImportError::NoTables);
    }
    tables.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(SourceSpec {
        name,
        base_url,
        auth: opts.auth,
        tables,
    })
}

/// The JSON schema of a successful (200 or first 2xx) JSON response.
fn success_schema(doc: &Json, op: &Json) -> Option<Json> {
    let responses = op.get("responses")?.as_object()?;
    let resp = responses.get("200").or_else(|| {
        responses
            .iter()
            .find(|(k, _)| k.starts_with('2'))
            .map(|(_, v)| v)
    })?;
    let resp = resolve(doc, resp);
    let schema = resp.pointer("/content/application~1json/schema")?;
    Some(resolve(doc, schema).clone())
}

/// Given a response schema, find the array of records and where it lives.
fn array_item_schema(doc: &Json, schema: &Json) -> Option<(RowPath, Json)> {
    let schema = resolve(doc, schema);
    if schema.get("type").and_then(Json::as_str) == Some("array") {
        let items = resolve(doc, schema.get("items")?);
        return Some((RowPath::Root, items.clone()));
    }

    // Wrapper object: use a single array property (prefer common names).
    let props = schema.get("properties").and_then(Json::as_object)?;
    let preferred = ["data", "items", "results", "records", "values"];
    let name = preferred
        .into_iter()
        .find(|n| props.contains_key(*n))
        .map(str::to_string)
        .or_else(|| {
            props
                .iter()
                .find(|(_, v)| resolve(doc, v).get("type").and_then(Json::as_str) == Some("array"))
                .map(|(k, _)| k.clone())
        })?;

    let arr = resolve(doc, props.get(&name)?);
    if arr.get("type").and_then(Json::as_str) != Some("array") {
        return None;
    }
    let items = resolve(doc, arr.get("items")?);
    Some((
        RowPath::Pointer {
            pointer: format!("/{name}"),
        },
        items.clone(),
    ))
}

/// Follow local `$ref`s (`#/...`) up to a small depth; return the node itself if
/// it isn't a ref or can't be resolved.
fn resolve<'a>(doc: &'a Json, node: &'a Json) -> &'a Json {
    let mut cur = node;
    for _ in 0..16 {
        let Some(reference) = cur.get("$ref").and_then(Json::as_str) else {
            break;
        };
        let Some(pointer) = reference.strip_prefix('#') else {
            break;
        };
        let Some(target) = doc.pointer(pointer) else {
            break;
        };
        cur = target;
    }
    cur
}

/// Map an OpenAPI property schema to a neutral column type. Composed schemas
/// (`anyOf`/`oneOf`/`allOf`, e.g. Stripe's expandable "id-or-object" fields) use
/// their first scalar branch, so an id string stays `Text` rather than `Json`.
fn schema_datatype(doc: &Json, sch: &Json) -> DataType {
    let sch = resolve(doc, sch);
    if let Some(dt) = scalar_datatype(sch) {
        return dt;
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = sch.get(key).and_then(Json::as_array) {
            for branch in branches {
                if let Some(dt) = scalar_datatype(resolve(doc, branch)) {
                    return dt;
                }
            }
        }
    }
    DataType::Json // object, array, or unknown
}

/// A schema's own scalar type, if it has one (`None` for object/array/composed).
fn scalar_datatype(sch: &Json) -> Option<DataType> {
    match sch.get("type").and_then(Json::as_str)? {
        "integer" => Some(DataType::Integer),
        "number" => Some(DataType::Float),
        "boolean" => Some(DataType::Bool),
        "string" => Some(match sch.get("format").and_then(Json::as_str) {
            Some("date-time") | Some("date") => DataType::Timestamp,
            _ => DataType::Text,
        }),
        _ => None,
    }
}

/// The names of an operation's `query` parameters.
fn query_params(op: &Json) -> Vec<String> {
    op.get("parameters")
        .and_then(Json::as_array)
        .map(|arr| {
            arr.iter()
                .filter(|p| p.get("in").and_then(Json::as_str) == Some("query"))
                .filter_map(|p| p.get("name").and_then(Json::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The last path segment, sanitized (e.g. `/api/v1/posts` → `posts`).
/// Returns `None` when there is no usable segment (e.g. the root path).
fn last_segment(path: &str) -> Option<String> {
    let seg = path.trim_matches('/').rsplit('/').next().unwrap_or("");
    let s = sanitize(seg);
    (!s.is_empty()).then_some(s)
}

/// Lowercase and reduce to `[a-z0-9_]`, collapsing separators.
fn sanitize(s: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_sep = false;
        } else if !prev_sep && !out.is_empty() {
            out.push('_');
            prev_sep = true;
        }
    }
    out.trim_end_matches('_').to_string()
}

/// Ensure a unique table name, appending `_2`, `_3`, … on collision.
fn dedup(name: String, used: &mut Vec<String>) -> String {
    let name = if name.is_empty() {
        "table".to_string()
    } else {
        name
    };
    if !used.contains(&name) {
        used.push(name.clone());
        return name;
    }
    let mut i = 2;
    loop {
        let candidate = format!("{name}_{i}");
        if !used.contains(&candidate) {
            used.push(candidate.clone());
            return candidate;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn array_resp(items_ref: &str) -> Json {
        json!({"responses": {"200": {"content": {"application/json": {"schema":
            {"type": "array", "items": {"$ref": items_ref}}}}}}})
    }

    #[test]
    fn imports_arrays_wrappers_types_and_filters() {
        let doc = json!({
            "info": {"title": "My API"},
            "servers": [{"url": "https://api.example.com"}],
            "paths": {
                "/posts": {
                    "get": {
                        "parameters": [
                            {"name": "userId", "in": "query", "schema": {"type": "integer"}},
                            {"name": "unknown", "in": "query", "schema": {"type": "string"}}
                        ],
                        "responses": {"200": {"content": {"application/json": {"schema":
                            {"type": "array", "items": {"$ref": "#/components/schemas/Post"}}}}}}
                    }
                },
                "/orders": {
                    "get": {
                        "parameters": [{"name": "h", "in": "header", "schema": {"type": "string"}}],
                        "responses": {"200": {"content": {"application/json": {"schema":
                            {"type": "array", "items": {"type": "object", "properties": {
                                "n": {"type": "integer"}, "amt": {"type": "number"},
                                "ok": {"type": "boolean"}, "s": {"type": "string"},
                                "ts": {"type": "string", "format": "date-time"},
                                "d": {"type": "string", "format": "date"},
                                "obj": {"type": "object"}, "arr": {"type": "array"},
                                "any": {}
                            }}}}}}}
                    }
                },
                "/wrapped": {
                    "get": {"responses": {"200": {"content": {"application/json": {"schema":
                        {"type": "object", "properties": {
                            "data": {"type": "array", "items": {"$ref": "#/components/schemas/Post"}},
                            "meta": {"type": "object"}}}}}}}}
                },
                "/oddwrap": {
                    "get": {"responses": {"200": {"content": {"application/json": {"schema":
                        {"type": "object", "properties": {
                            "list": {"type": "array", "items": {"type": "object",
                                "properties": {"x": {"type": "integer"}}}}}}}}}}}
                }
            },
            "components": {"schemas": {"Post": {"type": "object", "properties": {
                "id": {"type": "integer"}, "userId": {"type": "integer"}, "title": {"type": "string"}}}}}
        });

        let spec = SourceSpec::from_openapi(&doc, ImportOptions::default()).unwrap();
        assert_eq!(spec.name, "my_api");
        assert_eq!(spec.base_url, "https://api.example.com");

        let posts = spec.table("posts").unwrap();
        assert!(matches!(posts.row_path, RowPath::Root));
        // Only the query param matching a column is pushed down.
        assert_eq!(posts.filters.len(), 1);
        assert_eq!(posts.filters[0].param, "userId");
        assert_eq!(posts.columns.len(), 3);

        // Every OpenAPI type maps correctly (columns are sorted by name).
        let orders = spec.table("orders").unwrap();
        let ty = |name: &str| {
            orders
                .columns
                .iter()
                .find(|c| c.name == name)
                .unwrap()
                .data_type
        };
        assert_eq!(ty("n"), DataType::Integer);
        assert_eq!(ty("amt"), DataType::Float);
        assert_eq!(ty("ok"), DataType::Bool);
        assert_eq!(ty("s"), DataType::Text);
        assert_eq!(ty("ts"), DataType::Timestamp);
        assert_eq!(ty("d"), DataType::Timestamp);
        assert_eq!(ty("obj"), DataType::Json);
        assert_eq!(ty("arr"), DataType::Json);
        assert_eq!(ty("any"), DataType::Json);
        assert!(orders.filters.is_empty()); // header param is not pushdown

        // Wrapper objects use a pointer row path.
        assert!(matches!(
            spec.table("wrapped").unwrap().row_path,
            RowPath::Pointer { .. }
        ));
        assert!(matches!(
            spec.table("oddwrap").unwrap().row_path,
            RowPath::Pointer { .. }
        ));
    }

    #[test]
    fn skips_everything_that_is_not_a_collection() {
        let doc = json!({
            "servers": [{"url": "https://x"}],
            "paths": {
                "/users/{id}": {"get": array_resp("#/components/schemas/Post")}, // path param
                "/noget": {"post": {"responses": {}}},                            // no GET
                "/badresp": {"get": {"responses": {"404": {}}}},                  // no 2xx
                "/noschema": {"get": {"responses": {"200": {}}}},                 // no schema
                "/nolist": {"get": {"responses": {"200": {"content": {"application/json":
                    {"schema": {"type": "object", "properties": {"x": {"type": "integer"}}}}}}}}},
                "/fakewrap": {"get": {"responses": {"200": {"content": {"application/json":
                    {"schema": {"type": "object", "properties": {"data": {"type": "string"}}}}}}}}},
                "/strings": {"get": {"responses": {"200": {"content": {"application/json":
                    {"schema": {"type": "array", "items": {"type": "string"}}}}}}}},
                "/empty": {"get": {"responses": {"200": {"content": {"application/json":
                    {"schema": {"type": "array", "items": {"type": "object", "properties": {}}}}}}}}},
                "/ext": {"get": array_resp("external.json#/Foo")},                // external ref
                "/missingref": {"get": array_resp("#/components/schemas/Nope")}   // dangling ref
            }
        });
        let err = SourceSpec::from_openapi(&doc, ImportOptions::default()).unwrap_err();
        assert!(matches!(err, ImportError::NoTables));
        assert!(err.to_string().contains("no queryable"));
    }

    #[test]
    fn names_from_path_with_operation_id_fallback_and_dedup() {
        let doc = json!({
            "servers": [{"url": "https://x"}],
            "paths": {
                "/": {"get": {"operationId": "listRoot",
                    "responses": {"200": {"content": {"application/json": {"schema":
                        {"type": "array", "items": {"type": "object",
                            "properties": {"a": {"type": "integer"}}}}}}}}}},
                "/posts": {"get": {"responses": {"200": {"content": {"application/json": {"schema":
                    {"type": "array", "items": {"type": "object",
                        "properties": {"id": {"type": "integer"}}}}}}}}}},
                "/v2/posts": {"get": {"responses": {"200": {"content": {"application/json": {"schema":
                    {"type": "array", "items": {"type": "object",
                        "properties": {"id": {"type": "integer"}}}}}}}}}},
                "/v3/posts": {"get": {"responses": {"200": {"content": {"application/json": {"schema":
                    {"type": "array", "items": {"type": "object",
                        "properties": {"id": {"type": "integer"}}}}}}}}}},
                // No usable segment and no operationId → falls back to "table".
                "//": {"get": {"responses": {"200": {"content": {"application/json": {"schema":
                    {"type": "array", "items": {"type": "object",
                        "properties": {"id": {"type": "integer"}}}}}}}}}}
            }
        });
        let spec = SourceSpec::from_openapi(&doc, ImportOptions::default()).unwrap();
        let names: Vec<&str> = spec.tables.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"listroot")); // fallback to operationId for "/"
        assert!(names.contains(&"posts"));
        assert!(names.contains(&"posts_2"));
        assert!(names.contains(&"posts_3"));
        assert!(names.contains(&"table")); // empty name -> "table"
    }

    #[test]
    fn composed_schemas_map_to_first_scalar_branch() {
        // cust -> Text (first scalar of anyOf), amt -> Integer (oneOf),
        // code -> Text (allOf), blob -> Json (no scalar branch).
        let props = json!({
            "cust": {"anyOf": [{"type": "object"}, {"type": "string"}]},
            "amt":  {"oneOf": [{"type": "integer"}]},
            "code": {"allOf": [{"type": "string"}]},
            "blob": {"anyOf": [{"type": "object"}, {"type": "array"}]}
        });
        let items = json!({"type": "object", "properties": props});
        let schema = json!({"type": "array", "items": items});
        let doc = json!({
            "servers": [{"url": "https://x"}],
            "paths": {"/t": {"get": {"responses": {"200": {"content":
                {"application/json": {"schema": schema}}}}}}}
        });
        let spec = SourceSpec::from_openapi(&doc, ImportOptions::default()).unwrap();
        let t = spec.table("t").unwrap();
        let ty = |n: &str| t.columns.iter().find(|c| c.name == n).unwrap().data_type;
        assert_eq!(ty("cust"), DataType::Text);
        assert_eq!(ty("amt"), DataType::Integer);
        assert_eq!(ty("code"), DataType::Text);
        assert_eq!(ty("blob"), DataType::Json);
    }

    #[test]
    fn detects_cursor_pagination_and_include_filters_tables() {
        let doc = json!({
            "servers": [{"url": "https://api.stripe.com"}],
            "paths": {
                "/v1/customers": {"get": {
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer"}},
                        {"name": "starting_after", "in": "query", "schema": {"type": "string"}}
                    ],
                    "responses": {"200": {"content": {"application/json": {"schema":
                        {"type": "object", "properties": {
                            "data": {"type": "array", "items": {"type": "object",
                                "properties": {"id": {"type": "string"}}}},
                            "has_more": {"type": "boolean"}}}}}}}
                }},
                "/v1/charges": {"get": {"responses": {"200": {"content": {"application/json":
                    {"schema": {"type": "array", "items": {"type": "object",
                        "properties": {"id": {"type": "string"}}}}}}}}}}
            }
        });
        // include only "customers" → charges is skipped.
        let opts = ImportOptions {
            include: Some(vec!["customers".to_string()]),
            ..Default::default()
        };
        let spec = SourceSpec::from_openapi(&doc, opts).unwrap();
        assert_eq!(spec.tables.len(), 1);
        let customers = spec.table("customers").unwrap();
        assert!(matches!(customers.pagination, Pagination::Cursor { .. }));
        assert!(matches!(customers.row_path, RowPath::Pointer { .. }));
    }

    #[test]
    fn accepts_first_2xx_response_when_no_200() {
        let doc = json!({
            "servers": [{"url": "https://x"}],
            "paths": {"/created": {"get": {"responses": {"201": {"content": {"application/json":
                {"schema": {"type": "array", "items": {"type": "object",
                    "properties": {"id": {"type": "integer"}}}}}}}}}}}
        });
        let spec = SourceSpec::from_openapi(&doc, ImportOptions::default()).unwrap();
        assert!(spec.table("created").is_some());
    }

    #[test]
    fn options_override_name_and_base_url_and_auth() {
        let doc = json!({
            "info": {"title": "Ignored"},
            "servers": [{"url": "https://ignored"}],
            "paths": {"/t": {"get": {"responses": {"200": {"content": {"application/json":
                {"schema": {"type": "array", "items": {"type": "object",
                    "properties": {"id": {"type": "integer"}}}}}}}}}}}
        });
        let opts = ImportOptions {
            name: Some("custom".into()),
            base_url: Some("https://override".into()),
            auth: AuthSpec::Bearer { token: "t".into() },
            include: None,
        };
        let spec = SourceSpec::from_openapi(&doc, opts.clone()).unwrap();
        assert_eq!(spec.name, "custom");
        assert_eq!(spec.base_url, "https://override");
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        // Exercise Debug/Clone on options.
        assert!(!format!("{opts:?}").is_empty());
    }

    #[test]
    fn errors_on_malformed_documents() {
        // Missing servers and no override.
        let no_server = json!({"paths": {}});
        let e = SourceSpec::from_openapi(&no_server, ImportOptions::default()).unwrap_err();
        assert!(matches!(e, ImportError::Invalid(_)));
        assert!(e.to_string().contains("invalid OpenAPI"));

        // Missing paths.
        let no_paths = json!({"servers": [{"url": "https://x"}]});
        assert!(matches!(
            SourceSpec::from_openapi(&no_paths, ImportOptions::default()).unwrap_err(),
            ImportError::Invalid(_)
        ));

        // Bad JSON string.
        assert!(matches!(
            SourceSpec::from_openapi_json("not json", ImportOptions::default()).unwrap_err(),
            ImportError::Invalid(_)
        ));
    }

    #[test]
    fn from_openapi_json_parses_a_valid_document() {
        let doc = r#"{
            "servers": [{"url": "https://x"}],
            "paths": {"/t": {"get": {"responses": {"200": {"content": {"application/json":
                {"schema": {"type": "array", "items": {"type": "object",
                    "properties": {"id": {"type": "integer"}}}}}}}}}}}
        }"#;
        let spec = SourceSpec::from_openapi_json(doc, ImportOptions::default()).unwrap();
        assert_eq!(spec.name, "openapi"); // no info.title -> default
        assert!(spec.table("t").is_some());
    }
}
