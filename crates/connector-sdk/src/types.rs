//! The common data shapes every connector speaks in.
//!
//! A connector's job is to turn some external API into these neutral types.
//! Later, the Postgres FDW layer turns THESE into Postgres rows. Keeping a
//! single neutral representation in the middle is what lets one framework
//! drive every connector.

use serde::{Deserialize, Serialize};

/// The column types we support, roughly matching Postgres types.
///
/// `#[derive(...)]` asks the compiler to auto-generate common behavior:
/// - `Debug` lets us print it for debugging (`{:?}`)
/// - `Clone`/`Copy` let us duplicate the value cheaply
/// - `Serialize`/`Deserialize` (from `serde`) let it convert to/from JSON
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    Text,
    Integer,
    Float,
    Bool,
    Timestamp,
    Json,
}

/// One column: its name and what type of value it holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub data_type: DataType,
}

/// The shape of one table a connector exposes, e.g. "issues" with its columns.
/// This is what "schema discovery" produces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<Column>,
}

impl TableSchema {
    /// Small helper to build a schema tersely in connector code.
    pub fn new(name: &str, columns: Vec<Column>) -> Self {
        Self {
            name: name.to_string(),
            columns,
        }
    }
}

/// A single cell's value. This is an `enum`: a value that is exactly ONE of
/// these variants at a time. `Null` models a missing value (Postgres NULL).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    Null,
    Text(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Timestamp(String),
    Json(serde_json::Value),
}

impl Value {
    /// Render a value as a plain string, for printing tables in the CLI.
    pub fn to_display_string(&self) -> String {
        match self {
            Value::Null => "NULL".to_string(),
            Value::Text(s) => s.clone(),
            Value::Integer(n) => n.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Timestamp(s) => s.clone(),
            Value::Json(j) => j.to_string(),
        }
    }
}

/// One row of data: a list of values, positionally aligned with a
/// `TableSchema`'s columns (the i-th value belongs to the i-th column).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row(pub Vec<Value>);

/// A comparison operator in a filter. Mirrors SQL's `=`, `!=`, `<`, `>`, etc.
/// `Like` is a substring/text match (SQL `LIKE`, Jira `~`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
}

/// One filter condition, e.g. `status = 'Open'` → column="status", op=Eq,
/// value=Text("Open"). This is a single "qual" (qualifier) from a SQL `WHERE`.
#[derive(Debug, Clone)]
pub struct Filter {
    pub column: String,
    pub op: Operator,
    pub value: Value,
}

impl Filter {
    pub fn new(column: &str, op: Operator, value: Value) -> Self {
        Self {
            column: column.to_string(),
            op,
            value,
        }
    }
}

/// One sort key, e.g. `ORDER BY created DESC` → column="created", descending=true.
#[derive(Debug, Clone)]
pub struct SortKey {
    pub column: String,
    pub descending: bool,
}

/// What the caller wants back from a `fetch`. This is the neutral form of a SQL
/// query's shape: which rows (`filters`), in what order (`sort`), which columns
/// (`projection`), and how many (`limit`). A connector pushes as much of this
/// as its API supports down to the source — that's "pushdown".
#[derive(Debug, Clone, Default)]
pub struct Query {
    /// WHERE conditions. Empty = no filtering.
    pub filters: Vec<Filter>,
    /// ORDER BY keys. Empty = source's default order.
    pub sort: Vec<SortKey>,
    /// Which columns are needed. `None` = all columns.
    pub projection: Option<Vec<String>>,
    /// Row cap (SQL `LIMIT`).
    pub limit: Option<usize>,
}

impl Query {
    /// A short, stable string identifying this query — part of the cache key.
    /// It MUST reflect everything that changes the result (filters, sort,
    /// projection, limit), so different queries never share a cache entry.
    pub fn cache_key(&self) -> String {
        let filters = self
            .filters
            .iter()
            .map(|f| format!("{}{:?}{}", f.column, f.op, f.value.to_display_string()))
            .collect::<Vec<_>>()
            .join(",");
        let sort = self
            .sort
            .iter()
            .map(|s| format!("{}{}", s.column, if s.descending { "-" } else { "+" }))
            .collect::<Vec<_>>()
            .join(",");
        let projection = match &self.projection {
            Some(cols) => cols.join("+"),
            None => "*".to_string(),
        };
        let limit = match self.limit {
            Some(n) => n.to_string(),
            None => "all".to_string(),
        };
        format!("f[{filters}]|s[{sort}]|p[{projection}]|l[{limit}]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_display_covers_every_variant() {
        assert_eq!(Value::Null.to_display_string(), "NULL");
        assert_eq!(Value::Text("hi".into()).to_display_string(), "hi");
        assert_eq!(Value::Integer(42).to_display_string(), "42");
        assert_eq!(Value::Float(1.5).to_display_string(), "1.5");
        assert_eq!(Value::Bool(true).to_display_string(), "true");
        assert_eq!(
            Value::Timestamp("2026-01-01T00:00:00Z".into()).to_display_string(),
            "2026-01-01T00:00:00Z"
        );
        assert_eq!(
            Value::Json(serde_json::json!({"a": 1})).to_display_string(),
            "{\"a\":1}"
        );
    }

    #[test]
    fn table_schema_new_builds_expected_shape() {
        let schema = TableSchema::new(
            "issues",
            vec![
                Column {
                    name: "key".into(),
                    data_type: DataType::Text,
                },
                Column {
                    name: "n".into(),
                    data_type: DataType::Integer,
                },
            ],
        );
        assert_eq!(schema.name, "issues");
        assert_eq!(schema.columns.len(), 2);
        assert_eq!(schema.columns[0].data_type, DataType::Text);
    }

    #[test]
    fn filter_new_sets_fields() {
        let f = Filter::new("status", Operator::Eq, Value::Text("Open".into()));
        assert_eq!(f.column, "status");
        assert_eq!(f.op, Operator::Eq);
        assert_eq!(f.value.to_display_string(), "Open");
    }

    #[test]
    fn cache_key_default_is_stable() {
        let q = Query::default();
        assert_eq!(q.cache_key(), "f[]|s[]|p[*]|l[all]");
    }

    #[test]
    fn cache_key_reflects_all_query_parts() {
        let q = Query {
            filters: vec![
                Filter::new("project", Operator::Eq, Value::Text("ENG".into())),
                Filter::new("count", Operator::Gt, Value::Integer(3)),
            ],
            sort: vec![
                SortKey {
                    column: "created".into(),
                    descending: true,
                },
                SortKey {
                    column: "key".into(),
                    descending: false,
                },
            ],
            projection: Some(vec!["key".into(), "summary".into()]),
            limit: Some(10),
        };
        assert_eq!(
            q.cache_key(),
            "f[projectEqENG,countGt3]|s[created-,key+]|p[key+summary]|l[10]"
        );
    }

    #[test]
    fn every_operator_is_distinct_in_debug() {
        // Exercises the derived Debug/Copy/PartialEq on Operator.
        let ops = [
            Operator::Eq,
            Operator::Ne,
            Operator::Gt,
            Operator::Gte,
            Operator::Lt,
            Operator::Lte,
            Operator::Like,
        ];
        for op in ops {
            let copied = op; // Copy
            assert_eq!(op, copied);
            assert!(!format!("{op:?}").is_empty());
        }
    }

    #[test]
    fn neutral_types_round_trip_through_json() {
        // Exercises the Serialize/Deserialize/Debug/Clone derives on the neutral
        // data types, covering every Value variant.
        let row = Row(vec![
            Value::Null,
            Value::Text("t".into()),
            Value::Integer(1),
            Value::Float(2.0),
            Value::Bool(false),
            Value::Timestamp("2026".into()),
            Value::Json(serde_json::json!([1, 2])),
        ]);
        let schema = TableSchema::new(
            "t",
            vec![Column {
                name: "c".into(),
                data_type: DataType::Json,
            }],
        );

        let row_json = serde_json::to_string(&row).unwrap();
        let row_back: Row = serde_json::from_str(&row_json).unwrap();
        assert_eq!(row_back.0.len(), 7);
        assert!(!format!("{row_back:?}").is_empty());

        let schema_json = serde_json::to_string(&schema).unwrap();
        let schema_back: TableSchema = serde_json::from_str(&schema_json).unwrap();
        assert_eq!(schema_back.name, "t");
        // Clone + Debug on the surviving types.
        let _ = schema_back.clone();
        assert!(!format!("{:?}", DataType::Text).is_empty());
    }

    #[test]
    fn sort_key_and_query_are_debuggable_and_cloneable() {
        let q = Query {
            limit: Some(1),
            ..Default::default()
        };
        let _ = q.clone();
        assert!(!format!("{q:?}").is_empty());
        let sk = SortKey {
            column: "x".into(),
            descending: false,
        };
        let _ = sk.clone();
        assert!(!format!("{sk:?}").is_empty());
    }
}
