//! # connector-sdk
//!
//! The reusable framework for building budbuk connectors. It defines the
//! neutral data types, the error type, and the `Connector` trait that every
//! data source implements.
//!
//! `lib.rs` is the root of a *library* crate. It declares which modules exist
//! and what the crate exposes to the outside world.

// `mod` declares a module, whose code lives in the matching `.rs` file.
// `pub` makes it visible to code outside this crate.
pub mod cache;
pub mod connector;
pub mod error;
pub mod types;

// Re-exports: pull the most-used items up to the crate root so users can write
// `connector_sdk::Connector` instead of `connector_sdk::connector::Connector`.
pub use cache::{Cache, CachedConnector};
pub use connector::Connector;
pub use error::{ConnectorError, Result};
pub use types::{Column, DataType, Filter, Operator, Query, Row, SortKey, TableSchema, Value};
