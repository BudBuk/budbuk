//! # rest-connector
//!
//! A config-driven REST connector engine for BudBuk. One engine
//! ([`RestConnector`]) drives *any* REST API from a declarative [`SourceSpec`],
//! which can be hand-written (a specific connector) or generated from an OpenAPI
//! document (the "force multiplier"). Because it implements the
//! `connector_sdk::Connector` trait, it plugs into caching, tracing, and the
//! PostgreSQL FDW just like a bespoke connector.

pub mod cli;
pub mod connector;
pub mod openapi;
pub mod spec;

pub use connector::RestConnector;
pub use openapi::{ImportError, ImportOptions};
pub use spec::{AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec};
