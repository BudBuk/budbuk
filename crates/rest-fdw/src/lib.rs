//! BudBuk generic REST Foreign Data Wrapper.
//!
//! A single PostgreSQL extension that exposes *any* BudBuk `SourceSpec` as
//! foreign tables. The server's `spec` option carries a serialized `SourceSpec`
//! (hand-written, from a connector like GitHub, or generated from OpenAPI);
//! this FDW deserializes it, builds a `RestConnector`, and forwards Postgres
//! scans to it — pushing quals/columns/sort/limit down to the API.

use std::collections::HashMap;

use pgrx::pg_sys::panic::ErrorReport;
use pgrx::PgSqlErrorCode;
use supabase_wrappers::prelude::*;

use connector_sdk::{
    create_foreign_table_statements, Connector, Filter, ImportFilter, Operator, Query, SortKey,
    Value as CValue,
};
use rest_connector::{RestConnector, SourceSpec};

pgrx::pg_module_magic!(name, version);

#[derive(Debug)]
enum RestFdwError {
    Options(OptionsError),
    Config(String),
    Connector(String),
    Runtime(String),
}

impl From<OptionsError> for RestFdwError {
    fn from(e: OptionsError) -> Self {
        RestFdwError::Options(e)
    }
}

impl From<RestFdwError> for ErrorReport {
    fn from(e: RestFdwError) -> Self {
        let msg = match e {
            RestFdwError::Options(err) => err.to_string(),
            RestFdwError::Config(m) => format!("invalid spec: {m}"),
            RestFdwError::Connector(m) => format!("connector error: {m}"),
            RestFdwError::Runtime(m) => format!("async runtime error: {m}"),
        };
        ErrorReport::new(PgSqlErrorCode::ERRCODE_FDW_ERROR, msg, "")
    }
}

type RestFdwResult<T> = Result<T, RestFdwError>;

#[wrappers_fdw(
    version = "0.1.0",
    author = "The BudBuk Authors",
    website = "https://github.com/budbuk/budbuk",
    error_type = "RestFdwError"
)]
pub(crate) struct RestFdw {
    connector: RestConnector,
    schema_cols: Vec<connector_sdk::Column>,
    rows: Vec<connector_sdk::Row>,
    cursor: usize,
    tgt_cols: Vec<Column>,
}

impl ForeignDataWrapper<RestFdwError> for RestFdw {
    /// Build the engine from the server options. Either name a built-in
    /// connector (`connector 'stripe'`, plus its credentials) and the catalog
    /// supplies the bundled spec, or pass a raw serialized `SourceSpec` via
    /// `spec` for a fully custom source.
    fn new(server: ForeignServer) -> RestFdwResult<Self> {
        let spec: SourceSpec = if let Some(name) = server.options.get("connector") {
            catalog::spec_for(name, &server.options)
                .map_err(|e| RestFdwError::Config(e.to_string()))?
        } else {
            let spec_json = require_option("spec", &server.options)?;
            serde_json::from_str(spec_json).map_err(|e| RestFdwError::Config(e.to_string()))?
        };
        Ok(Self {
            connector: RestConnector::new(spec),
            schema_cols: Vec::new(),
            rows: Vec::new(),
            cursor: 0,
            tgt_cols: Vec::new(),
        })
    }

    /// `IMPORT FOREIGN SCHEMA` — auto-create a foreign table for every table
    /// the connector discovers, so users don't hand-write DDL. Honors
    /// `LIMIT TO` / `EXCEPT`.
    fn import_foreign_schema(
        &mut self,
        stmt: ImportForeignSchemaStmt,
    ) -> RestFdwResult<Vec<String>> {
        let rt = create_async_runtime().map_err(|e| RestFdwError::Runtime(e.to_string()))?;
        let schemas = rt
            .block_on(self.connector.discover())
            .map_err(|e| RestFdwError::Connector(e.to_string()))?;
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
    ) -> RestFdwResult<()> {
        let table = require_option("object", options)?.to_string();

        let rt = create_async_runtime().map_err(|e| RestFdwError::Runtime(e.to_string()))?;
        let schemas = rt
            .block_on(self.connector.discover())
            .map_err(|e| RestFdwError::Connector(e.to_string()))?;
        let schema = schemas
            .into_iter()
            .find(|s| s.name == table)
            .ok_or_else(|| RestFdwError::Connector(format!("unknown table: {table}")))?;
        self.schema_cols = schema.columns;

        let query = build_query(quals, columns, sorts, limit);
        self.rows = rt
            .block_on(self.connector.fetch(&table, &query))
            .map_err(|e| RestFdwError::Connector(e.to_string()))?;
        self.cursor = 0;
        self.tgt_cols = columns.to_vec();
        Ok(())
    }

    fn iter_scan(&mut self, row: &mut Row) -> RestFdwResult<Option<()>> {
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

    fn re_scan(&mut self) -> RestFdwResult<()> {
        self.cursor = 0;
        Ok(())
    }

    fn end_scan(&mut self) -> RestFdwResult<()> {
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
    ) -> RestFdwResult<(i64, i32)> {
        Ok((1000, 256))
    }
}

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
