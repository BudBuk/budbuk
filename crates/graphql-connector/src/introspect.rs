//! The introspection generator: turn a GraphQL schema (the result of the
//! standard introspection query) into a [`GraphQlSpec`]. This is the analog of
//! the REST engine's OpenAPI importer.
//!
//! Heuristic (root `Query` fields only):
//! - a field returning a **Relay connection** (`edges { node }`) → a table with
//!   [`NodeShape::Connection`] and Relay pagination;
//! - a field returning a **list of objects** → a table with [`NodeShape::List`];
//! - anything else (scalars, single objects) → skipped.
//!
//! Node columns come from the node type's fields: scalars/enums become typed
//! columns; a nested object becomes a single `Json` column (its scalar leaves
//! are selected one level deep so the value is actually fetched). Scalar/enum
//! field arguments become filter variables (equality pushdown).

use std::collections::HashMap;

use connector_sdk::DataType;
use serde::Deserialize;

use crate::spec::{
    AuthSpec, ColumnSpec, FilterVar, GraphQlSpec, GraphQlTable, NodeShape, Pagination,
};

/// Options controlling generation.
pub struct ImportOptions {
    /// The GraphQL endpoint the generated spec will POST to.
    pub endpoint: String,
    /// Auth for the generated spec.
    pub auth: AuthSpec,
    /// If set, only generate tables for these root field names.
    pub include: Option<Vec<String>>,
    /// Page size for generated Relay pagination.
    pub page_size: usize,
    /// Spec name (defaults to `"graphql"`).
    pub name: Option<String>,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            auth: AuthSpec::None,
            include: None,
            page_size: 50,
            name: None,
        }
    }
}

/// Why generation failed.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("invalid introspection JSON: {0}")]
    Parse(String),
    #[error("no __schema found in the introspection document")]
    NoSchema,
    #[error("schema has no query root type")]
    NoQueryType,
    #[error("no list/connection fields found on the query root")]
    Empty,
}

// ── Minimal typed view of an introspection document ────────────────────────

#[derive(Deserialize)]
struct Schema {
    #[serde(rename = "queryType")]
    query_type: Option<NamedRef>,
    types: Vec<FullType>,
}

#[derive(Deserialize)]
struct NamedRef {
    name: Option<String>,
}

#[derive(Deserialize)]
struct FullType {
    name: Option<String>,
    #[serde(default)]
    fields: Option<Vec<Field>>,
}

#[derive(Deserialize)]
struct Field {
    name: String,
    #[serde(default)]
    args: Vec<InputValue>,
    #[serde(rename = "type")]
    type_ref: TypeRef,
}

#[derive(Deserialize)]
struct InputValue {
    name: String,
    #[serde(rename = "type")]
    type_ref: TypeRef,
}

#[derive(Deserialize, Clone)]
struct TypeRef {
    kind: String,
    name: Option<String>,
    #[serde(rename = "ofType")]
    of_type: Option<Box<TypeRef>>,
}

impl GraphQlSpec {
    /// Generate a spec from a GraphQL introspection document (JSON string).
    pub fn from_introspection_json(
        doc: &str,
        options: ImportOptions,
    ) -> std::result::Result<GraphQlSpec, ImportError> {
        let root: serde_json::Value =
            serde_json::from_str(doc).map_err(|e| ImportError::Parse(e.to_string()))?;
        // The __schema may sit under `data`, at the top level, or the doc may be
        // the schema object itself.
        let schema_val = root
            .pointer("/data/__schema")
            .or_else(|| root.pointer("/__schema"))
            .unwrap_or(&root);
        let schema: Schema =
            serde_json::from_value(schema_val.clone()).map_err(|_| ImportError::NoSchema)?;

        let query_type_name = schema
            .query_type
            .as_ref()
            .and_then(|q| q.name.as_deref())
            .ok_or(ImportError::NoQueryType)?
            .to_string();

        // Index object-ish types by name for lookups.
        let index: HashMap<&str, &FullType> = schema
            .types
            .iter()
            .filter_map(|t| t.name.as_deref().map(|n| (n, t)))
            .collect();

        let query_type = index
            .get(query_type_name.as_str())
            .ok_or(ImportError::NoQueryType)?;
        let query_fields = query_type.fields.as_deref().unwrap_or(&[]);

        let mut tables = Vec::new();
        for field in query_fields {
            if let Some(inc) = &options.include {
                if !inc.iter().any(|n| n == &field.name) {
                    continue;
                }
            }
            if let Some(table) = build_table(field, &index, options.page_size) {
                tables.push(table);
            }
        }

        if tables.is_empty() {
            return Err(ImportError::Empty);
        }

        Ok(GraphQlSpec {
            name: options.name.unwrap_or_else(|| "graphql".to_string()),
            endpoint: options.endpoint,
            auth: options.auth,
            tables,
        })
    }
}

/// Build a table from one root query field, or `None` if it isn't a
/// list/connection of objects.
fn build_table(
    field: &Field,
    index: &HashMap<&str, &FullType>,
    page_size: usize,
) -> Option<GraphQlTable> {
    let (named, is_list) = unwrap_named(&field.type_ref);
    let obj = named.name.as_deref().and_then(|n| index.get(n))?;

    // Non-pagination scalar/enum args become filter variables.
    let (arg_decls, arg_calls, filters) = field_arguments(field);

    if let Some(node_name) = connection_node(obj, index) {
        // Relay connection.
        let node = index.get(node_name.as_str())?;
        let (selection, columns) = node_selection(node, index);
        if columns.is_empty() {
            return None;
        }
        let mut decls = vec!["$first: Int".to_string(), "$after: String".to_string()];
        decls.extend(arg_decls);
        let mut calls = vec!["first: $first".to_string(), "after: $after".to_string()];
        calls.extend(arg_calls);
        let query = format!(
            "query {name}({decls}) {{ {name}({calls}) {{ edges {{ node {{ {selection} }} }} pageInfo {{ hasNextPage endCursor }} }} }}",
            name = field.name,
            decls = decls.join(", "),
            calls = calls.join(", "),
        );
        Some(GraphQlTable {
            name: field.name.clone(),
            query,
            data_pointer: format!("/{}", field.name),
            shape: NodeShape::Connection,
            columns,
            pagination: Pagination::Relay {
                first_var: "first".to_string(),
                after_var: "after".to_string(),
                page_size,
            },
            filters,
        })
    } else if is_list {
        // Plain list of objects.
        let (selection, columns) = node_selection(obj, index);
        if columns.is_empty() {
            return None;
        }
        let decls = if arg_decls.is_empty() {
            String::new()
        } else {
            format!("({})", arg_decls.join(", "))
        };
        let calls = if arg_calls.is_empty() {
            String::new()
        } else {
            format!("({})", arg_calls.join(", "))
        };
        let query = format!(
            "query {name}{decls} {{ {name}{calls} {{ {selection} }} }}",
            name = field.name,
        );
        Some(GraphQlTable {
            name: field.name.clone(),
            query,
            data_pointer: format!("/{}", field.name),
            shape: NodeShape::List,
            columns,
            pagination: Pagination::None,
            filters,
        })
    } else {
        None
    }
}

/// If `obj` is a Relay connection (has `edges` whose edge type has a `node`),
/// return the node object's type name.
fn connection_node(obj: &FullType, index: &HashMap<&str, &FullType>) -> Option<String> {
    let edges = obj.fields.as_ref()?.iter().find(|f| f.name == "edges")?;
    let (edge_named, _) = unwrap_named(&edges.type_ref);
    let edge_obj = index.get(edge_named.name.as_deref()?)?;
    let node = edge_obj
        .fields
        .as_ref()?
        .iter()
        .find(|f| f.name == "node")?;
    let (node_named, _) = unwrap_named(&node.type_ref);
    node_named.name.clone()
}

/// Build the GraphQL selection set and matching columns for a node type.
/// Scalars/enums → typed columns; nested objects → one level of scalar leaves,
/// mapped to a single `Json` column.
fn node_selection(node: &FullType, index: &HashMap<&str, &FullType>) -> (String, Vec<ColumnSpec>) {
    let mut selection = Vec::new();
    let mut columns = Vec::new();
    let fields = node.fields.as_deref().unwrap_or(&[]);
    for f in fields {
        // Fields that require arguments can't be selected bare.
        if f.args.iter().any(|a| a.type_ref.kind == "NON_NULL") {
            continue;
        }
        let (named, _) = unwrap_named(&f.type_ref);
        match named.kind.as_str() {
            "SCALAR" | "ENUM" => {
                selection.push(f.name.clone());
                columns.push(ColumnSpec {
                    name: f.name.clone(),
                    field: f.name.clone(),
                    data_type: map_scalar(named.name.as_deref().unwrap_or("String")),
                });
            }
            "OBJECT" => {
                // One level deep: select the nested object's scalar leaves.
                let sub = named.name.as_deref().and_then(|n| index.get(n));
                let leaves: Vec<String> = sub
                    .and_then(|o| o.fields.as_deref())
                    .unwrap_or(&[])
                    .iter()
                    .filter(|sf| {
                        !sf.args.iter().any(|a| a.type_ref.kind == "NON_NULL")
                            && is_scalarish(&unwrap_named(&sf.type_ref).0.kind)
                    })
                    .map(|sf| sf.name.clone())
                    .collect();
                if !leaves.is_empty() {
                    selection.push(format!("{} {{ {} }}", f.name, leaves.join(" ")));
                    columns.push(ColumnSpec {
                        name: f.name.clone(),
                        field: f.name.clone(),
                        data_type: DataType::Json,
                    });
                }
            }
            _ => {} // INTERFACE / UNION / nested connections: skipped.
        }
    }
    (selection.join(" "), columns)
}

/// Turn a field's scalar/enum arguments into variable declarations, call
/// arguments, and filter mappings. Pagination and input-object args are skipped.
fn field_arguments(field: &Field) -> (Vec<String>, Vec<String>, Vec<FilterVar>) {
    let mut decls = Vec::new();
    let mut calls = Vec::new();
    let mut filters = Vec::new();
    for a in &field.args {
        if matches!(a.name.as_str(), "first" | "after" | "last" | "before") {
            continue;
        }
        let (named, _) = unwrap_named(&a.type_ref);
        if is_scalarish(&named.kind) {
            decls.push(format!("${}: {}", a.name, type_ref_to_gql(&a.type_ref)));
            calls.push(format!("{}: ${}", a.name, a.name));
            filters.push(FilterVar {
                column: a.name.clone(),
                variable: a.name.clone(),
            });
        }
    }
    (decls, calls, filters)
}

/// Walk `NON_NULL`/`LIST` wrappers to the innermost named type; report whether a
/// list was crossed.
fn unwrap_named(t: &TypeRef) -> (&TypeRef, bool) {
    let mut cur = t;
    let mut is_list = false;
    loop {
        match cur.kind.as_str() {
            "NON_NULL" => match &cur.of_type {
                Some(inner) => cur = inner,
                None => break,
            },
            "LIST" => {
                is_list = true;
                match &cur.of_type {
                    Some(inner) => cur = inner,
                    None => break,
                }
            }
            _ => break,
        }
    }
    (cur, is_list)
}

/// Render a type reference as a GraphQL type string, e.g. `[String!]!`.
fn type_ref_to_gql(t: &TypeRef) -> String {
    match t.kind.as_str() {
        "NON_NULL" => format!(
            "{}!",
            t.of_type
                .as_ref()
                .map(|i| type_ref_to_gql(i))
                .unwrap_or_default()
        ),
        "LIST" => format!(
            "[{}]",
            t.of_type
                .as_ref()
                .map(|i| type_ref_to_gql(i))
                .unwrap_or_default()
        ),
        _ => t.name.clone().unwrap_or_default(),
    }
}

fn is_scalarish(kind: &str) -> bool {
    kind == "SCALAR" || kind == "ENUM"
}

/// Map a GraphQL scalar name to a neutral column type.
fn map_scalar(name: &str) -> DataType {
    match name {
        "Int" => DataType::Integer,
        "Float" => DataType::Float,
        "Boolean" => DataType::Bool,
        "DateTime" | "Date" | "Time" | "Timestamp" => DataType::Timestamp,
        _ => DataType::Text, // ID, String, custom scalars, enums.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A rich schema exercising: a connection field with a scalar arg, a list
    // field with a nested object (→ Json) and an enum, a single-object field
    // (skipped), a scalar field (skipped), a connection whose node has no
    // usable columns (skipped), and a node field requiring an argument (skipped).
    fn schema_doc() -> String {
        serde_json::json!({
            "data": {"__schema": {
                "queryType": {"name": "Query"},
                "types": [
                    {"name": "Query", "fields": [
                        {"name": "issues", "args": [
                            {"name": "first", "type": {"kind": "SCALAR", "name": "Int", "ofType": null}},
                            {"name": "state", "type": {"kind": "SCALAR", "name": "String", "ofType": null}},
                            {"name": "where", "type": {"kind": "INPUT_OBJECT", "name": "Filter", "ofType": null}}
                        ], "type": {"kind": "OBJECT", "name": "IssueConnection", "ofType": null}},
                        {"name": "countries", "args": [], "type": {"kind": "NON_NULL", "name": null, "ofType":
                            {"kind": "LIST", "name": null, "ofType":
                                {"kind": "OBJECT", "name": "Country", "ofType": null}}}},
                        {"name": "viewer", "args": [], "type": {"kind": "OBJECT", "name": "User", "ofType": null}},
                        {"name": "version", "args": [], "type": {"kind": "SCALAR", "name": "String", "ofType": null}},
                        {"name": "empties", "args": [], "type": {"kind": "LIST", "name": null, "ofType":
                            {"kind": "OBJECT", "name": "Empty", "ofType": null}}}
                    ]},
                    {"name": "IssueConnection", "fields": [
                        {"name": "edges", "args": [], "type": {"kind": "LIST", "name": null, "ofType":
                            {"kind": "OBJECT", "name": "IssueEdge", "ofType": null}}},
                        {"name": "pageInfo", "args": [], "type": {"kind": "OBJECT", "name": "PageInfo", "ofType": null}}
                    ]},
                    {"name": "IssueEdge", "fields": [
                        {"name": "node", "args": [], "type": {"kind": "OBJECT", "name": "Issue", "ofType": null}}
                    ]},
                    {"name": "Issue", "fields": [
                        {"name": "id", "args": [], "type": {"kind": "SCALAR", "name": "ID", "ofType": null}},
                        {"name": "count", "args": [], "type": {"kind": "SCALAR", "name": "Int", "ofType": null}},
                        {"name": "createdAt", "args": [], "type": {"kind": "SCALAR", "name": "DateTime", "ofType": null}},
                        {"name": "secret", "args": [
                            {"name": "key", "type": {"kind": "NON_NULL", "name": null, "ofType":
                                {"kind": "SCALAR", "name": "String", "ofType": null}}}
                        ], "type": {"kind": "SCALAR", "name": "String", "ofType": null}}
                    ]},
                    {"name": "Country", "fields": [
                        {"name": "code", "args": [], "type": {"kind": "SCALAR", "name": "ID", "ofType": null}},
                        {"name": "score", "args": [], "type": {"kind": "SCALAR", "name": "Float", "ofType": null}},
                        {"name": "active", "args": [], "type": {"kind": "SCALAR", "name": "Boolean", "ofType": null}},
                        {"name": "kind", "args": [], "type": {"kind": "ENUM", "name": "Kind", "ofType": null}},
                        {"name": "continent", "args": [], "type": {"kind": "OBJECT", "name": "Continent", "ofType": null}},
                        {"name": "tags", "args": [], "type": {"kind": "INTERFACE", "name": "Node", "ofType": null}}
                    ]},
                    {"name": "Continent", "fields": [
                        {"name": "code", "args": [], "type": {"kind": "SCALAR", "name": "String", "ofType": null}},
                        {"name": "sub", "args": [], "type": {"kind": "OBJECT", "name": "Deep", "ofType": null}}
                    ]},
                    {"name": "User", "fields": [
                        {"name": "login", "args": [], "type": {"kind": "SCALAR", "name": "String", "ofType": null}}
                    ]},
                    {"name": "Empty", "fields": [
                        {"name": "rel", "args": [], "type": {"kind": "OBJECT", "name": "OnlyObjects", "ofType": null}}
                    ]},
                    {"name": "OnlyObjects", "fields": [
                        {"name": "child", "args": [], "type": {"kind": "OBJECT", "name": "Deep", "ofType": null}}
                    ]},
                    {"name": "Deep", "fields": [
                        {"name": "leaf", "args": [], "type": {"kind": "OBJECT", "name": "Deep", "ofType": null}}
                    ]}
                ]
            }}
        })
        .to_string()
    }

    #[test]
    fn generates_connection_and_list_tables() {
        let spec = GraphQlSpec::from_introspection_json(
            &schema_doc(),
            ImportOptions {
                endpoint: "https://x/graphql".into(),
                page_size: 25,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(spec.name, "graphql");
        // Only issues (connection) and countries (list) become tables.
        // viewer (single object), version (scalar), empties (no usable columns) are skipped.
        let names: Vec<&str> = spec.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["issues", "countries"]);

        let issues = spec.table("issues").unwrap();
        assert_eq!(issues.shape, NodeShape::Connection);
        assert!(matches!(
            issues.pagination,
            Pagination::Relay { page_size: 25, .. }
        ));
        // scalar arg `state` → filter var; input-object arg `where` skipped; `first` skipped.
        assert_eq!(issues.filters.len(), 1);
        assert_eq!(issues.filters[0].variable, "state");
        assert!(issues.query.contains("$first: Int"));
        assert!(issues.query.contains("$state: String"));
        assert!(issues.query.contains("edges { node {"));
        assert!(issues.query.contains("pageInfo { hasNextPage endCursor }"));
        // node columns: id (ID→Text), count (Int→Integer), createdAt (DateTime→Timestamp);
        // `secret` requires an arg → skipped.
        let cols: Vec<&str> = issues.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(cols, vec!["id", "count", "createdAt"]);
        assert_eq!(issues.columns[0].data_type, DataType::Text);
        assert_eq!(issues.columns[1].data_type, DataType::Integer);
        assert_eq!(issues.columns[2].data_type, DataType::Timestamp);

        let countries = spec.table("countries").unwrap();
        assert_eq!(countries.shape, NodeShape::List);
        assert!(matches!(countries.pagination, Pagination::None));
        // code(ID→Text), score(Float), active(Bool), kind(enum→Text), continent(object→Json);
        // tags(interface) skipped.
        let cols: Vec<(&str, DataType)> = countries
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c.data_type))
            .collect();
        assert_eq!(
            cols,
            vec![
                ("code", DataType::Text),
                ("score", DataType::Float),
                ("active", DataType::Bool),
                ("kind", DataType::Text),
                ("continent", DataType::Json),
            ]
        );
        // Nested object selected one level deep (its scalar `code`, not `sub`).
        assert!(countries.query.contains("continent { code }"));
        assert!(!countries.query.contains("sub"));
        assert_eq!(countries.data_pointer, "/countries");
    }

    #[test]
    fn include_filter_limits_tables() {
        let spec = GraphQlSpec::from_introspection_json(
            &schema_doc(),
            ImportOptions {
                endpoint: "https://x".into(),
                include: Some(vec!["countries".into()]),
                name: Some("c".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(spec.name, "c");
        assert_eq!(spec.tables.len(), 1);
        assert_eq!(spec.tables[0].name, "countries");
    }

    #[test]
    fn accepts_schema_at_top_level_and_bare() {
        // __schema at the top level (no `data` wrapper).
        let top = serde_json::json!({"__schema": {
            "queryType": {"name": "Query"},
            "types": [
                {"name": "Query", "fields": [
                    {"name": "countries", "args": [], "type": {"kind": "LIST", "name": null,
                        "ofType": {"kind": "OBJECT", "name": "Country", "ofType": null}}}
                ]},
                {"name": "Country", "fields": [
                    {"name": "code", "args": [], "type": {"kind": "SCALAR", "name": "String", "ofType": null}}
                ]}
            ]
        }})
        .to_string();
        let spec = GraphQlSpec::from_introspection_json(&top, ImportOptions::default()).unwrap();
        assert_eq!(spec.tables.len(), 1);

        // The doc *is* the schema object.
        let bare = serde_json::json!({
            "queryType": {"name": "Query"},
            "types": [
                {"name": "Query", "fields": [
                    {"name": "countries", "args": [], "type": {"kind": "LIST", "name": null,
                        "ofType": {"kind": "OBJECT", "name": "Country", "ofType": null}}}
                ]},
                {"name": "Country", "fields": [
                    {"name": "code", "args": [], "type": {"kind": "SCALAR", "name": "String", "ofType": null}}
                ]}
            ]
        })
        .to_string();
        let spec2 = GraphQlSpec::from_introspection_json(&bare, ImportOptions::default()).unwrap();
        assert_eq!(spec2.tables.len(), 1);
    }

    #[test]
    fn error_paths() {
        // Bad JSON.
        assert!(matches!(
            GraphQlSpec::from_introspection_json("not json", ImportOptions::default()),
            Err(ImportError::Parse(_))
        ));
        // Valid JSON but not a schema shape (types missing) → NoSchema.
        assert!(matches!(
            GraphQlSpec::from_introspection_json("{\"data\":{}}", ImportOptions::default()),
            Err(ImportError::NoSchema)
        ));
        // No query type.
        let no_query = serde_json::json!({"types": []}).to_string();
        assert!(matches!(
            GraphQlSpec::from_introspection_json(&no_query, ImportOptions::default()),
            Err(ImportError::NoQueryType)
        ));
        // Query type named but not present in `types`.
        let missing_query_type =
            serde_json::json!({"queryType": {"name": "Query"}, "types": []}).to_string();
        assert!(matches!(
            GraphQlSpec::from_introspection_json(&missing_query_type, ImportOptions::default()),
            Err(ImportError::NoQueryType)
        ));
        // Query type present but yields no tables → Empty.
        let empty = serde_json::json!({
            "queryType": {"name": "Query"},
            "types": [{"name": "Query", "fields": [
                {"name": "version", "args": [], "type": {"kind": "SCALAR", "name": "String", "ofType": null}}
            ]}]
        })
        .to_string();
        assert!(matches!(
            GraphQlSpec::from_introspection_json(&empty, ImportOptions::default()),
            Err(ImportError::Empty)
        ));
        // Every ImportError renders a message.
        for e in [
            ImportError::Parse("x".into()),
            ImportError::NoSchema,
            ImportError::NoQueryType,
            ImportError::Empty,
        ] {
            assert!(!e.to_string().is_empty());
            assert!(!format!("{e:?}").is_empty());
        }
    }

    #[test]
    fn query_type_with_no_fields_is_empty() {
        // Query type exists in the index but has null fields.
        let doc = serde_json::json!({
            "queryType": {"name": "Query"},
            "types": [{"name": "Query", "fields": null}]
        })
        .to_string();
        assert!(matches!(
            GraphQlSpec::from_introspection_json(&doc, ImportOptions::default()),
            Err(ImportError::Empty)
        ));
    }

    #[test]
    fn connection_with_no_columns_is_skipped() {
        // A connection whose node has only object fields with no scalar leaves.
        let doc = serde_json::json!({
            "queryType": {"name": "Query"},
            "types": [
                {"name": "Query", "fields": [
                    {"name": "things", "args": [], "type": {"kind": "OBJECT", "name": "ThingConnection", "ofType": null}}
                ]},
                {"name": "ThingConnection", "fields": [
                    {"name": "edges", "args": [], "type": {"kind": "LIST", "name": null,
                        "ofType": {"kind": "OBJECT", "name": "ThingEdge", "ofType": null}}}
                ]},
                {"name": "ThingEdge", "fields": [
                    {"name": "node", "args": [], "type": {"kind": "OBJECT", "name": "Thing", "ofType": null}}
                ]},
                {"name": "Thing", "fields": [
                    {"name": "rel", "args": [], "type": {"kind": "OBJECT", "name": "Missing", "ofType": null}}
                ]}
            ]
        })
        .to_string();
        assert!(matches!(
            GraphQlSpec::from_introspection_json(&doc, ImportOptions::default()),
            Err(ImportError::Empty)
        ));
    }

    #[test]
    fn helpers_handle_wrappers_and_malformed_refs() {
        // type_ref_to_gql over NON_NULL[LIST[NON_NULL ID]].
        let t = TypeRef {
            kind: "NON_NULL".into(),
            name: None,
            of_type: Some(Box::new(TypeRef {
                kind: "LIST".into(),
                name: None,
                of_type: Some(Box::new(TypeRef {
                    kind: "NON_NULL".into(),
                    name: None,
                    of_type: Some(Box::new(TypeRef {
                        kind: "SCALAR".into(),
                        name: Some("ID".into()),
                        of_type: None,
                    })),
                })),
            })),
        };
        assert_eq!(type_ref_to_gql(&t), "[ID!]!");
        let (named, is_list) = unwrap_named(&t);
        assert!(is_list);
        assert_eq!(named.name.as_deref(), Some("ID"));

        // Malformed: wrapper with no ofType.
        let bad_nn = TypeRef {
            kind: "NON_NULL".into(),
            name: None,
            of_type: None,
        };
        assert_eq!(type_ref_to_gql(&bad_nn), "!");
        assert_eq!(unwrap_named(&bad_nn).0.kind, "NON_NULL");
        let bad_list = TypeRef {
            kind: "LIST".into(),
            name: None,
            of_type: None,
        };
        assert_eq!(type_ref_to_gql(&bad_list), "[]");
        let (n, is_list) = unwrap_named(&bad_list);
        assert!(is_list);
        assert_eq!(n.kind, "LIST");
        // A bare named type with no name renders empty.
        let anon = TypeRef {
            kind: "SCALAR".into(),
            name: None,
            of_type: None,
        };
        assert_eq!(type_ref_to_gql(&anon), "");
    }

    #[test]
    fn list_field_with_scalar_arg_gets_variables_and_filters() {
        // A plain list field that takes a scalar argument exercises the
        // non-empty declaration/call formatting and produces a filter.
        let doc = serde_json::json!({
            "queryType": {"name": "Query"},
            "types": [
                {"name": "Query", "fields": [
                    {"name": "search", "args": [
                        {"name": "term", "type": {"kind": "NON_NULL", "name": null, "ofType":
                            {"kind": "SCALAR", "name": "String", "ofType": null}}}
                    ], "type": {"kind": "LIST", "name": null, "ofType":
                        {"kind": "OBJECT", "name": "Hit", "ofType": null}}}
                ]},
                {"name": "Hit", "fields": [
                    {"name": "id", "args": [], "type": {"kind": "SCALAR", "name": "ID", "ofType": null}}
                ]}
            ]
        })
        .to_string();
        let spec = GraphQlSpec::from_introspection_json(
            &doc,
            ImportOptions {
                endpoint: "https://x".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let t = spec.table("search").unwrap();
        assert_eq!(t.shape, NodeShape::List);
        assert_eq!(t.filters.len(), 1);
        assert_eq!(t.filters[0].variable, "term");
        // Non-empty decls and calls are wrapped in parens.
        assert!(t.query.contains("search($term: String!)"));
        assert!(t.query.contains("search(term: $term)"));
    }

    #[test]
    fn map_scalar_covers_every_arm() {
        assert_eq!(map_scalar("Int"), DataType::Integer);
        assert_eq!(map_scalar("Float"), DataType::Float);
        assert_eq!(map_scalar("Boolean"), DataType::Bool);
        assert_eq!(map_scalar("DateTime"), DataType::Timestamp);
        assert_eq!(map_scalar("Date"), DataType::Timestamp);
        assert_eq!(map_scalar("Time"), DataType::Timestamp);
        assert_eq!(map_scalar("Timestamp"), DataType::Timestamp);
        assert_eq!(map_scalar("ID"), DataType::Text);
        assert_eq!(map_scalar("String"), DataType::Text);
        assert_eq!(map_scalar("CustomScalar"), DataType::Text);
        assert!(is_scalarish("SCALAR"));
        assert!(is_scalarish("ENUM"));
        assert!(!is_scalarish("OBJECT"));
    }
}
