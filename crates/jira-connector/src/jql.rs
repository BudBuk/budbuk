//! Translate neutral `Query` filters/sort into Jira's JQL — the "pushdown".
//!
//! Not every column is pushable (an API only understands some fields). We push
//! what we can and report which filters we pushed, so the caller can re-check
//! the rest locally. This "best-effort push + local recheck" is exactly how
//! PostgreSQL FDWs stay correct while still being fast.

use connector_sdk::{Filter, Operator, SortKey, Value};

/// Map a neutral column name to the JQL field name for the `issues` table.
/// Returns `None` for columns Jira's JQL can't filter on directly.
fn jql_field(column: &str) -> Option<&'static str> {
    match column {
        "key" => Some("key"),
        "status" => Some("status"),
        "assignee" => Some("assignee"),
        "project" => Some("project"),
        "summary" => Some("summary"),
        "created" => Some("created"),
        _ => None,
    }
}

/// The JQL spelling of an operator.
fn op_str(op: Operator) -> &'static str {
    match op {
        Operator::Eq => "=",
        Operator::Ne => "!=",
        Operator::Gt => ">",
        Operator::Gte => ">=",
        Operator::Lt => "<",
        Operator::Lte => "<=",
        Operator::Like => "~", // JQL text-contains
    }
}

/// Render a value for JQL: strings are quoted (and escaped); numbers/bools bare.
fn jql_value(v: &Value) -> String {
    match v {
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Text(s) | Value::Timestamp(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        Value::Null => "null".to_string(),
        Value::Json(j) => format!("\"{}\"", j),
    }
}

/// Result of building JQL: the query string, plus which of the caller's filters
/// we actually pushed (by index), so the caller knows what it must still apply.
pub struct BuiltJql {
    pub jql: String,
    pub pushed: Vec<usize>,
}

/// Build a JQL string from pushable filters + sort. JQL must be *bounded*, so
/// if nothing pushable remains we fall back to a default time window.
pub fn build_issues_jql(filters: &[Filter], sort: &[SortKey]) -> BuiltJql {
    let mut clauses = Vec::new();
    let mut pushed = Vec::new();

    for (i, f) in filters.iter().enumerate() {
        if let Some(field) = jql_field(&f.column) {
            clauses.push(format!(
                "{} {} {}",
                field,
                op_str(f.op),
                jql_value(&f.value)
            ));
            pushed.push(i);
        }
        // Unpushable filters are simply skipped here; the caller re-applies them.
    }

    // Bound the query if no clause was produced (Jira rejects unbounded JQL).
    let where_part = if clauses.is_empty() {
        "created >= -90d".to_string()
    } else {
        clauses.join(" AND ")
    };

    // ORDER BY from pushable sort keys, defaulting to newest-first.
    let order = sort
        .iter()
        .filter_map(|s| {
            jql_field(&s.column)
                .map(|field| format!("{} {}", field, if s.descending { "DESC" } else { "ASC" }))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let order = if order.is_empty() {
        "created DESC".to_string()
    } else {
        order
    };

    BuiltJql {
        jql: format!("{where_part} ORDER BY {order}"),
        pushed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use connector_sdk::{Filter, Operator, SortKey, Value};

    #[test]
    fn jql_field_maps_known_and_rejects_unknown() {
        for (col, want) in [
            ("key", "key"),
            ("status", "status"),
            ("assignee", "assignee"),
            ("project", "project"),
            ("summary", "summary"),
            ("created", "created"),
        ] {
            assert_eq!(jql_field(col), Some(want));
        }
        assert_eq!(jql_field("story_points"), None);
    }

    #[test]
    fn op_str_covers_every_operator() {
        assert_eq!(op_str(Operator::Eq), "=");
        assert_eq!(op_str(Operator::Ne), "!=");
        assert_eq!(op_str(Operator::Gt), ">");
        assert_eq!(op_str(Operator::Gte), ">=");
        assert_eq!(op_str(Operator::Lt), "<");
        assert_eq!(op_str(Operator::Lte), "<=");
        assert_eq!(op_str(Operator::Like), "~");
    }

    #[test]
    fn jql_value_renders_every_value_kind() {
        assert_eq!(jql_value(&Value::Integer(5)), "5");
        assert_eq!(jql_value(&Value::Float(1.5)), "1.5");
        assert_eq!(jql_value(&Value::Bool(true)), "true");
        assert_eq!(jql_value(&Value::Text("Open".into())), "\"Open\"");
        assert_eq!(jql_value(&Value::Timestamp("2026".into())), "\"2026\"");
        assert_eq!(jql_value(&Value::Null), "null");
        assert_eq!(jql_value(&Value::Json(serde_json::json!("x"))), "\"\"x\"\"");
        // Embedded quotes are escaped.
        assert_eq!(jql_value(&Value::Text("a\"b".into())), "\"a\\\"b\"");
    }

    #[test]
    fn empty_query_falls_back_to_bounded_default() {
        let built = build_issues_jql(&[], &[]);
        assert_eq!(built.jql, "created >= -90d ORDER BY created DESC");
        assert!(built.pushed.is_empty());
    }

    #[test]
    fn single_pushable_filter_is_translated() {
        let filters = vec![Filter::new(
            "project",
            Operator::Eq,
            Value::Text("ENG".into()),
        )];
        let built = build_issues_jql(&filters, &[]);
        assert_eq!(built.jql, "project = \"ENG\" ORDER BY created DESC");
        assert_eq!(built.pushed, vec![0]);
    }

    #[test]
    fn multiple_filters_are_joined_with_and_and_unpushable_are_skipped() {
        let filters = vec![
            Filter::new("story_points", Operator::Gt, Value::Integer(3)), // unpushable
            Filter::new("status", Operator::Eq, Value::Text("Open".into())),
            Filter::new("assignee", Operator::Ne, Value::Text("bob".into())),
        ];
        let built = build_issues_jql(&filters, &[]);
        assert_eq!(
            built.jql,
            "status = \"Open\" AND assignee != \"bob\" ORDER BY created DESC"
        );
        // Only indices 1 and 2 were pushed (0 was unpushable).
        assert_eq!(built.pushed, vec![1, 2]);
    }

    #[test]
    fn sort_keys_render_asc_and_desc_and_skip_unpushable() {
        let sort = vec![
            SortKey {
                column: "status".into(),
                descending: false,
            },
            SortKey {
                column: "created".into(),
                descending: true,
            },
            SortKey {
                column: "story_points".into(),
                descending: true,
            }, // skipped
        ];
        let built = build_issues_jql(&[], &sort);
        assert_eq!(
            built.jql,
            "created >= -90d ORDER BY status ASC, created DESC"
        );
    }

    #[test]
    fn built_jql_struct_is_usable() {
        let built = build_issues_jql(&[], &[]);
        // Field access on the public struct.
        assert!(built.jql.contains("ORDER BY"));
        assert_eq!(built.pushed.len(), 0);
    }
}
