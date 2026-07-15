# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **connector-sdk** crate: the reusable framework.
  - `Connector` trait (`name`, `discover`, `fetch`) — implement once per source.
  - Neutral data types: `Value`, `Row`, `Column`, `TableSchema`, `DataType`.
  - Query model with predicate pushdown support: `Query`, `Filter`, `Operator`, `SortKey`.
  - Typed error type `ConnectorError` and `Result` alias.
  - `Cache` + `CachedConnector`: an in-memory TTL cache with stale-while-revalidate,
    per-account namespacing, and a thundering-herd guard, applied via a decorator.
- **jira-connector** crate: the first connector.
  - Schema discovery for `projects`, `issues`, `users`, and `worklogs`.
  - Async Jira Cloud REST client (`reqwest` + `serde`) with typed JSON parsing.
  - Pagination for both token-based (`nextPageToken`) and offset-based (`startAt`) APIs.
  - Predicate pushdown: `WHERE` / `ORDER BY` / `LIMIT` translated to JQL, with
    best-effort push and reporting of un-pushable filters.
  - Mock mode: runs with built-in sample data and no credentials.
  - Demo CLI (`jira-cli`) showing discovery, caching, and pushdown.
- **Observability**: structured logging/tracing via the `tracing` crate.
  - Cache events (`hit`/`miss`/`stale`/`expired`) and HTTP request timing
    (`url`, `status`, `elapsed_ms`) as structured `tracing` events.
  - Cache metrics counters exposed via `CachedConnector::metrics()`
    (`CacheMetricsSnapshot`: hits, misses, stale, expired, refreshes).
  - CLI installs a `tracing-subscriber`; log level is controlled by `RUST_LOG`
    (e.g. `RUST_LOG=budbuk::cache=debug`).
- **PostgreSQL FDW** (`crates/jira-fdw`): a `pgrx` + `supabase-wrappers` extension
  that exposes Jira as foreign tables, so you can `SELECT` from Jira in `psql`.
  - Forwards Postgres scan callbacks to the `JiraConnector` engine.
  - Pushes `WHERE` / `ORDER BY` / `LIMIT` and column projection down to JQL.
  - Excluded from the main Cargo workspace (built/run via `cargo pgrx`), keeping
    the engine's `cargo build/test` and the 100%-coverage gate independent.
  - Example setup SQL in `crates/jira-fdw/sql/example.sql`.
- Project infrastructure: dual MIT/Apache-2.0 license, README and community docs,
  GitHub Actions CI (fmt, clippy, tests on stable + MSRV, 100% line-coverage gate,
  `cargo-deny`, docs), release workflow, Dependabot, and a `Makefile` of dev tasks.

[Unreleased]: https://github.com/budbuk/budbuk/commits/main
