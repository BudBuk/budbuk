//! The `Connector` trait — the contract every data source implements.
//!
//! A `trait` in Rust is a shared interface: a list of capabilities a type
//! promises to provide. Any type that implements `Connector` can be driven by
//! the framework identically — that is EXACTLY how "add a new data source with
//! minimal code" works. You implement these three methods for Jira; tomorrow
//! you implement them for GitHub; the surrounding machinery never changes.

use async_trait::async_trait;

use crate::error::Result;
use crate::types::{Query, Row, TableSchema};

/// Anything that can expose external data as queryable tables.
///
/// `#[async_trait]` is a small helper macro. Rust's async-in-traits has sharp
/// edges when used through pointers (which the framework needs), so this macro
/// smooths them over. Think of it as "this trait has async methods."
///
/// `: Send + Sync` are *bounds*: they promise a connector is safe to move
/// between threads and share across them. The async runtime needs this to run
/// connectors concurrently.
#[async_trait]
pub trait Connector: Send + Sync {
    /// A short, stable identifier for this connector, e.g. `"jira"`.
    fn name(&self) -> &str;

    /// Schema discovery: list every table this connector exposes, each with
    /// its columns and their types. The FDW layer will turn these into
    /// PostgreSQL foreign tables.
    ///
    /// `async` means this may do I/O (e.g. ask the API what fields exist)
    /// without blocking the thread. It returns a `Result` because it can fail
    /// (network down, bad credentials, …).
    async fn discover(&self) -> Result<Vec<TableSchema>>;

    /// Fetch rows from one table, honoring the `query` (its `limit` for now;
    /// filters/projection/sort in Step 2). `&self` means it borrows the
    /// connector immutably — it reads config but doesn't modify the connector.
    async fn fetch(&self, table: &str, query: &Query) -> Result<Vec<Row>>;
}
