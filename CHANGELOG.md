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
- Project infrastructure: dual MIT/Apache-2.0 license, README and community docs,
  GitHub Actions CI (fmt, clippy, tests on stable + MSRV, 100% line-coverage gate,
  `cargo-deny`, docs), release workflow, Dependabot, and a `Makefile` of dev tasks.

[Unreleased]: https://github.com/budbuk/budbuk/commits/main
