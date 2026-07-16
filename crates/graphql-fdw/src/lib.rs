//! BudBuk generic GraphQL Foreign Data Wrapper.
//!
//! A single PostgreSQL extension that exposes *any* BudBuk `GraphQlSpec` as
//! foreign tables. The server's `spec` option carries a serialized `GraphQlSpec`
//! (hand-written or generated from schema introspection); this FDW deserializes
//! it, builds a `GraphQlConnector`, and forwards Postgres scans to it — pushing
//! quals/columns/sort/limit down to the API.
//!
//! Like the REST FDW, all HTTP goes through the workspace's rustls-backed
//! `reqwest`, so it is free of the native-tls fork-unsafe segfault.

use std::collections::HashMap;

use pgrx::pg_sys::panic::ErrorReport;
use pgrx::PgSqlErrorCode;
use supabase_wrappers::prelude::*;

use connector_sdk::{
    create_foreign_table_statements, Connector, Filter, ImportFilter, Operator, Query, SortKey,
    Value as CValue,
};
use graphql_connector::{GraphQlConnector, GraphQlSpec};

pgrx::pg_module_magic!(name, version);

#[derive(Debug)]
enum GraphQlFdwError {
    Options(OptionsError),
    Config(String),
    Connector(String),
    Runtime(String),
}

impl From<OptionsError> for GraphQlFdwError {
    fn from(e: OptionsError) -> Self {
        GraphQlFdwError::Options(e)
    }
}

impl From<GraphQlFdwError> for ErrorReport {
    fn from(e: GraphQlFdwError) -> Self {
        let msg = match e {
            GraphQlFdwError::Options(err) => err.to_string(),
            GraphQlFdwError::Config(m) => format!("invalid spec: {m}"),
            GraphQlFdwError::Connector(m) => format!("connector error: {m}"),
            GraphQlFdwError::Runtime(m) => format!("async runtime error: {m}"),
        };
        ErrorReport::new(PgSqlErrorCode::ERRCODE_FDW_ERROR, msg, "")
    }
}

type GraphQlFdwResult<T> = Result<T, GraphQlFdwError>;

#[wrappers_fdw(
    version = "0.1.0",
    author = "The BudBuk Authors",
    website = "https://github.com/budbuk/budbuk",
    error_type = "GraphQlFdwError"
)]
pub(crate) struct GraphqlFdw {
    connector: GraphQlConnector,
    schema_cols: Vec<connector_sdk::Column>,
    rows: Vec<connector_sdk::Row>,
    cursor: usize,
    tgt_cols: Vec<Column>,
}

impl ForeignDataWrapper<GraphQlFdwError> for GraphqlFdw {
    /// Build the engine from the server options: a serialized `GraphQlSpec` in
    /// the `spec` option (hand-written or generated from introspection).
    fn new(server: ForeignServer) -> GraphQlFdwResult<Self> {
        let spec_json = require_option("spec", &server.options)?;
        let spec: GraphQlSpec =
            serde_json::from_str(spec_json).map_err(|e| GraphQlFdwError::Config(e.to_string()))?;
        Ok(Self {
            connector: GraphQlConnector::new(spec),
            schema_cols: Vec::new(),
            rows: Vec::new(),
            cursor: 0,
            tgt_cols: Vec::new(),
        })
    }

    /// `IMPORT FOREIGN SCHEMA` — auto-create a foreign table for every table
    /// the GraphQL spec exposes. Honors `LIMIT TO` / `EXCEPT`.
    fn import_foreign_schema(
        &mut self,
        stmt: ImportForeignSchemaStmt,
    ) -> GraphQlFdwResult<Vec<String>> {
        let rt = create_async_runtime().map_err(|e| GraphQlFdwError::Runtime(e.to_string()))?;
        let schemas = rt
            .block_on(self.connector.discover())
            .map_err(|e| GraphQlFdwError::Connector(e.to_string()))?;
        let filter = match stmt.list_type {
            ImportSchemaType::FdwImportSchemaLimitTo => ImportFilter::LimitTo(stmt.table_list),
            ImportSchemaType::FdwImportSchemaExcept => ImportFilter::Except(stmt.table_list),
            ImportSchemaType::FdwImportSchemaAll => ImportFilter::All,
        };
        Ok(create_foreign_table_statements(
            &schemas,
            &stmt.server_name,
            &stmt.local_schema,
            &filter,
        ))
    }

    fn begin_scan(
        &mut self,
        quals: &[Qual],
        columns: &[Column],
        sorts: &[Sort],
        limit: &Option<Limit>,
        options: &HashMap<String, String>,
    ) -> GraphQlFdwResult<()> {
        let table = require_option("object", options)?.to_string();

        let rt = create_async_runtime().map_err(|e| GraphQlFdwError::Runtime(e.to_string()))?;
        let schemas = rt
            .block_on(self.connector.discover())
            .map_err(|e| GraphQlFdwError::Connector(e.to_string()))?;
        let schema = schemas
            .into_iter()
            .find(|s| s.name == table)
            .ok_or_else(|| GraphQlFdwError::Connector(format!("unknown table: {table}")))?;
        self.schema_cols = schema.columns;

        let query = build_query(quals, columns, sorts, limit);
        self.rows = rt
            .block_on(self.connector.fetch(&table, &query))
            .map_err(|e| GraphQlFdwError::Connector(e.to_string()))?;
        self.cursor = 0;
        self.tgt_cols = columns.to_vec();
        Ok(())
    }

    fn iter_scan(&mut self, row: &mut Row) -> GraphQlFdwResult<Option<()>> {
        if self.cursor >= self.rows.len() {
            return Ok(None);
        }
        let src = &self.rows[self.cursor];
        self.cursor += 1;

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

    fn re_scan(&mut self) -> GraphQlFdwResult<()> {
        self.cursor = 0;
        Ok(())
    }

    fn end_scan(&mut self) -> GraphQlFdwResult<()> {
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
    ) -> GraphQlFdwResult<(i64, i32)> {
        Ok((1000, 256))
    }
}

fn build_query(quals: &[Qual], columns: &[Column], sorts: &[Sort], limit: &Option<Limit>) -> Query {
    let filters = quals.iter().filter_map(qual_to_filter).collect();
    let sort = sorts
        .iter()
        .map(|s| SortKey {
            column: s.field.clone(),
            descending: s.reversed,
        })
        .collect();
    let projection = Some(columns.iter().map(|c| c.name.clone()).collect());
    let limit = limit.as_ref().map(|l| l.count.max(0) as usize);
    Query {
        filters,
        sort,
        projection,
        limit,
    }
}

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
    Some(Filter {
        column: q.field.clone(),
        op,
        value,
    })
}

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
