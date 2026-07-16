//! # graphql-connector
//!
//! A config-driven GraphQL connector engine for BudBuk. One engine
//! ([`GraphQlConnector`]) drives *any* GraphQL API from a declarative
//! [`GraphQlSpec`], which can be hand-written or generated from a schema
//! introspection document (the analog of the REST OpenAPI importer). Because it
//! implements the `connector_sdk::Connector` trait, it plugs into caching,
//! tracing, and the PostgreSQL FDW just like the REST engine.

pub mod cli;
pub mod connector;
pub mod introspect;
pub mod spec;

pub use connector::GraphQlConnector;
pub use introspect::{ImportError, ImportOptions};
pub use spec::{AuthSpec, ColumnSpec, FilterVar, GraphQlSpec, GraphQlTable, NodeShape, Pagination};
