//! BudBuk Jira Foreign Data Wrapper.
//!
//! A thin PostgreSQL FDW (built on `pgrx` + `supabase-wrappers`) that forwards
//! scans to the `jira-connector` engine. Postgres calls `begin_scan` /
//! `iter_scan` / `end_scan`; we translate its quals/columns/sort/limit into the
//! neutral `Query`, run the async connector via a blocking runtime, and convert
//! the returned rows back into Postgres cells.

use std::collections::HashMap;

use pgrx::pg_sys::panic::ErrorReport;
use pgrx::prelude::*;
use pgrx::PgSqlErrorCode;
use supabase_wrappers::prelude::*;

use connector_sdk::{Connector, Filter, Operator, Query, SortKey, Value as CValue};
use jira_connector::{JiraConfig, JiraConnector};

pgrx::pg_module_magic!(name, version);

// ---------------------------------------------------------------------------
// Error type (required by the `wrappers_fdw` macro)
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum JiraFdwError {
    Options(OptionsError),
    Connector(String),
    Runtime(String),
}

impl From<OptionsError> for JiraFdwError {
    fn from(e: OptionsError) -> Self {
        JiraFdwError::Options(e)
    }
}

impl From<JiraFdwError> for ErrorReport {
    fn from(e: JiraFdwError) -> Self {
        let msg = match e {
            JiraFdwError::Options(err) => err.to_string(),
            JiraFdwError::Connector(m) => format!("jira connector error: {m}"),
            JiraFdwError::Runtime(m) => format!("async runtime error: {m}"),
        };
        ErrorReport::new(PgSqlErrorCode::ERRCODE_FDW_ERROR, msg, "")
    }
}

type JiraFdwResult<T> = Result<T, JiraFdwError>;

// ---------------------------------------------------------------------------
// The FDW
// ---------------------------------------------------------------------------

#[wrappers_fdw(
    version = "0.1.0",
    author = "The BudBuk Authors",
    website = "https://github.com/budbuk/budbuk",
    error_type = "JiraFdwError"
)]
pub(crate) struct JiraFdw {
    /// The engine for this account (built from server options).
    connector: JiraConnector,
    /// The scanned table's full, ordered schema — lets us map a neutral row's
    /// positional values to the columns Postgres asked for.
    schema_cols: Vec<connector_sdk::Column>,
    /// The rows fetched for the current scan, and a cursor into them.
    rows: Vec<connector_sdk::Row>,
    cursor: usize,
    /// The columns Postgres wants back (projection).
    tgt_cols: Vec<Column>,
}

impl ForeignDataWrapper<JiraFdwError> for JiraFdw {
    /// Build the connector from the foreign server's options.
    ///
    /// For this PoC, credentials live in `CREATE SERVER ... OPTIONS`. A hardened
    /// deployment would source secrets from a secrets manager instead.
    fn new(server: ForeignServer) -> JiraFdwResult<Self> {
        let base_url = require_option("base_url", &server.options)?.to_string();
        let email = require_option("email", &server.options)?.to_string();
        let api_token = require_option("api_token", &server.options)?.to_string();

        let connector = JiraConnector::new(JiraConfig { base_url, email, api_token, mock: false });

        Ok(Self {
            connector,
            schema_cols: Vec::new(),
            rows: Vec::new(),
            cursor: 0,
            tgt_cols: Vec::new(),
        })
    }

    fn begin_scan(
        &mut self,
        quals: &[Qual],
        columns: &[Column],
        sorts: &[Sort],
        limit: &Option<Limit>,
        options: &HashMap<String, String>,
    ) -> JiraFdwResult<()> {
        // Which table? Set via `CREATE FOREIGN TABLE ... OPTIONS (object 'issues')`.
        let table = require_option("object", options)?.to_string();

        // A single-threaded runtime to drive the async connector synchronously.
        let rt = create_async_runtime().map_err(|e| JiraFdwError::Runtime(e.to_string()))?;

        // Discover the table's ordered schema so we can map values by position.
        let schemas = rt
            .block_on(self.connector.discover())
            .map_err(|e| JiraFdwError::Connector(e.to_string()))?;
        let schema = schemas
            .into_iter()
            .find(|s| s.name == table)
            .ok_or_else(|| JiraFdwError::Connector(format!("unknown table: {table}")))?;
        self.schema_cols = schema.columns;

        // Translate Postgres's scan shape into our neutral Query (pushdown).
        let query = build_query(quals, columns, sorts, limit);

        // Fetch. The connector handles pagination, JQL pushdown, retries, etc.
        self.rows = rt
            .block_on(self.connector.fetch(&table, &query))
            .map_err(|e| JiraFdwError::Connector(e.to_string()))?;
        self.cursor = 0;
        self.tgt_cols = columns.to_vec();
        Ok(())
    }

    fn iter_scan(&mut self, row: &mut Row) -> JiraFdwResult<Option<()>> {
        if self.cursor >= self.rows.len() {
            return Ok(None);
        }
        let src = &self.rows[self.cursor];
        self.cursor += 1;

        // Emit exactly the columns Postgres asked for, in its order.
        for col in &self.tgt_cols {
            let cell = self
                .schema_cols
                .iter()
                .position(|c| c.name == col.name)
                .and_then(|i| src.0.get(i))
                .and_then(value_to_cell);
            row.push(&col.name, cell);
        }
        Ok(Some(()))
    }

    fn re_scan(&mut self) -> JiraFdwResult<()> {
        self.cursor = 0;
        Ok(())
    }

    fn end_scan(&mut self) -> JiraFdwResult<()> {
        self.rows.clear();
        self.cursor = 0;
        Ok(())
    }

    fn get_rel_size(
        &mut self,
        _quals: &[Qual],
        _columns: &[Column],
        _sorts: &[Sort],
        _limit: &Option<Limit>,
        _options: &HashMap<String, String>,
    ) -> JiraFdwResult<(i64, i32)> {
        // Rough default estimate (rows, average width in bytes).
        Ok((1000, 256))
    }
}

// ---------------------------------------------------------------------------
// Conversions between the neutral engine types and Postgres cells
// ---------------------------------------------------------------------------

/// Build the neutral `Query` from Postgres's scan parameters (predicate/sort/
/// projection/limit pushdown).
fn build_query(quals: &[Qual], columns: &[Column], sorts: &[Sort], limit: &Option<Limit>) -> Query {
    let filters = quals.iter().filter_map(qual_to_filter).collect();
    let sort = sorts
        .iter()
        .map(|s| SortKey { column: s.field.clone(), descending: s.reversed })
        .collect();
    let projection = Some(columns.iter().map(|c| c.name.clone()).collect());
    let limit = limit.as_ref().map(|l| l.count.max(0) as usize);
    Query { filters, sort, projection, limit }
}

/// Convert one Postgres qual (`field op value`) into a neutral `Filter`.
/// Returns `None` for shapes we don't push down (e.g. array/OR quals) — those
/// stay for Postgres to re-check locally.
fn qual_to_filter(q: &Qual) -> Option<Filter> {
    if q.use_or {
        return None;
    }
    let op = match q.operator.as_str() {
        "=" => Operator::Eq,
        "<>" | "!=" => Operator::Ne,
        ">" => Operator::Gt,
        ">=" => Operator::Gte,
        "<" => Operator::Lt,
        "<=" => Operator::Lte,
        "~~" => Operator::Like,
        _ => return None,
    };
    let value = match &q.value {
        Value::Cell(cell) => cell_to_value(cell)?,
        Value::Array(_) => return None,
    };
    Some(Filter { column: q.field.clone(), op, value })
}

/// Postgres cell -> neutral value (for qual operands).
fn cell_to_value(cell: &Cell) -> Option<CValue> {
    Some(match cell {
        Cell::String(s) => CValue::Text(s.clone()),
        Cell::I64(n) => CValue::Integer(*n),
        Cell::I32(n) => CValue::Integer(*n as i64),
        Cell::I16(n) => CValue::Integer(*n as i64),
        Cell::I8(n) => CValue::Integer(*n as i64),
        Cell::F64(f) => CValue::Float(*f),
        Cell::F32(f) => CValue::Float(*f as f64),
        Cell::Bool(b) => CValue::Bool(*b),
        _ => return None,
    })
}

/// Neutral value -> Postgres cell (for scan output). `Null` becomes `None`.
/// Timestamps are emitted as text, so declare those columns `text` in the
/// foreign table.
fn value_to_cell(v: &CValue) -> Option<Cell> {
    match v {
        CValue::Null => None,
        CValue::Text(s) => Some(Cell::String(s.clone())),
        CValue::Integer(n) => Some(Cell::I64(*n)),
        CValue::Float(f) => Some(Cell::F64(*f)),
        CValue::Bool(b) => Some(Cell::Bool(*b)),
        CValue::Timestamp(s) => Some(Cell::String(s.clone())),
        CValue::Json(j) => Some(Cell::String(j.to_string())),
    }
}

/// Required by `cargo pgrx test`.
#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}

    #[must_use]
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec![]
    }
}
