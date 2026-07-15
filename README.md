<h1 align="center">BudBuk</h1>

<p align="center">
  <strong>A high-performance, PostgreSQL-native data integration platform in Rust.</strong><br>
  Query Jira, GitHub, Slack, and other SaaS sources with plain SQL — fast, cached, and safe.
</p>

<p align="center">
  <a href="https://github.com/budbuk/budbuk/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/budbuk/budbuk/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="Coverage" src="https://img.shields.io/badge/coverage-100%25%20lines-brightgreen">
  <img alt="License" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue">
  <img alt="MSRV" src="https://img.shields.io/badge/rustc-1.82%2B-orange">
</p>

---

> **Status: working proof of concept.** The connector engine (schema discovery, live
> REST fetching, pagination, caching, predicate pushdown, and observability) works
> today against a real Jira Cloud instance — and a PostgreSQL Foreign Data Wrapper
> (built on `pgrx`) lets you `SELECT` from it directly in `psql`. See
> [Querying from PostgreSQL](#querying-from-postgresql-the-fdw).

## What is BudBuk?

BudBuk lets you query external SaaS data **directly from PostgreSQL using standard
SQL**, as if each data source were a set of local tables:

```sql
SELECT key, summary, status, assignee
FROM jira_work.issues
WHERE project = 'ENG' AND status = 'Open'
ORDER BY created DESC
LIMIT 50;
```

Under the hood, BudBuk translates that query into the source's own API (for Jira,
into JQL), fetches only what's needed, caches it intelligently, and returns the rows
to PostgreSQL. You get a familiar SQL interface over data that lives behind rate-limited
web APIs — without writing a single line of glue code per query.

## Why?

- **One query language for everything.** Stop writing bespoke scripts per API. Join
  your Jira issues against local tables in SQL.
- **Fast by design.** Results are cached in memory (and, on the roadmap, in
  PostgreSQL) with TTL and stale-while-revalidate, so repeat queries are served in
  microseconds instead of hundreds of milliseconds.
- **Safe by design.** Slow or failing external systems are isolated so they can't
  degrade database stability. Written in Rust: memory-safe, concurrent, production-grade.
- **Easy to extend.** A reusable connector SDK standardizes auth, HTTP, retries,
  pagination, caching, and schema mapping. Adding a new source is mostly "implement
  one trait."

## Features

| Area | What you get |
|------|--------------|
| **Schema discovery** | Each connector exposes typed tables (columns + PostgreSQL-ish types). |
| **Live fetching** | Async HTTP (`reqwest` + `tokio`) with typed JSON parsing (`serde`). |
| **Pagination** | Handles both token-based (`nextPageToken`) and offset-based (`startAt`) paging. |
| **Caching** | In-memory TTL cache with **stale-while-revalidate** and a thundering-herd guard. |
| **Predicate pushdown** | Translates `WHERE` / `ORDER BY` / `LIMIT` into the source's query language (JQL for Jira). |
| **Multi-account** | One connector *type*, many *instances* (e.g. two Jira accounts), with isolated credentials and namespaced caches. |
| **Error handling** | Typed errors; network/auth/parse failures are contained, not fatal. |
| **100% line coverage** | Enforced in CI; see [Development](#development). |

## Architecture

BudBuk uses a **three-layer hybrid** design that combines the live feel of a real FDW,
the isolation of a sidecar, and the speed of materialization:

```
        ┌────────────────────────────────────────────────────────┐
        │  PostgreSQL   SELECT ... FROM jira_work.issues WHERE ... │
        └───────────────────────────┬────────────────────────────┘
                                     │  FDW callbacks (planned: pgrx)
        ┌────────────────────────────▼───────────────────────────┐
        │  FDW shim (thin)  — extracts quals/projection/sort/limit│   ← roadmap
        └───────────────────────────┬────────────────────────────┘
                                     │  scan request
        ┌────────────────────────────▼───────────────────────────┐
        │  Connector Engine (async Rust)                          │
        │   • connector-sdk: Connector trait, neutral types       │
        │   • cache: TTL + stale-while-revalidate (namespaced)    │
        │   • jira-connector: HTTP client, JQL pushdown, paging   │
        └───────────────────────────┬────────────────────────────┘
                                     │  cache miss → async HTTP
        ┌────────────────────────────▼───────────────────────────┐
        │  External API (Jira Cloud REST, GitHub, Slack, …)       │
        └─────────────────────────────────────────────────────────┘
```

The heavy work (fetching, caching, rate limiting) lives in a standalone async engine so
a slow API call never blocks a PostgreSQL backend. The FDW shim (planned) stays thin.

See [`docs/superpowers/specs/`](docs/superpowers/specs/) for the full design document.

## Repository layout

```
budbuk/
├── crates/
│   ├── connector-sdk/     # Reusable framework: Connector trait, neutral types, cache
│   │   └── src/
│   │       ├── connector.rs   # the Connector trait (implement once per source)
│   │       ├── types.rs        # Value, Row, Column, TableSchema, Query, Filter, ...
│   │       ├── error.rs        # ConnectorError + Result
│   │       └── cache.rs        # Cache + CachedConnector (TTL + SWR decorator)
│   ├── jira-connector/    # The Jira connector + demo CLI
│   │   └── src/
│   │       ├── lib.rs          # JiraConnector (implements Connector)
│   │       ├── client.rs       # async Jira REST client + pagination
│   │       ├── jql.rs          # WHERE/ORDER BY → JQL pushdown
│   │       ├── mock.rs         # canned sample data (no credentials needed)
│   │       ├── cli.rs          # CLI orchestration (tested)
│   │       └── main.rs         # thin binary entrypoint
│   ├── rest-connector/    # Config-driven REST engine (one engine, any API)
│   │   └── src/
│   │       ├── spec.rs         # SourceSpec: declarative API description
│   │       ├── connector.rs    # RestConnector (auth, pagination, pushdown)
│   │       ├── openapi.rs      # OpenAPI doc → SourceSpec importer
│   │       └── cli.rs          # demo against JSONPlaceholder (no auth)
│   ├── github-connector/  # GitHub as a SourceSpec over the engine (no HTTP code)
│   │   └── src/lib.rs          # github_spec(): repos, issues, gists, orgs
│   ├── jira-fdw/          # PostgreSQL FDW for Jira (pgrx; excluded from workspace)
│   │   ├── src/lib.rs          # ForeignDataWrapper → JiraConnector shim
│   │   └── sql/example.sql     # CREATE SERVER / FOREIGN TABLE example
│   └── rest-fdw/          # Generic PostgreSQL FDW: any SourceSpec → SQL (pgrx)
│       ├── src/lib.rs          # ForeignDataWrapper → RestConnector from a spec
│       └── sql/example.sql     # query GitHub from psql
├── docs/                  # design specs
├── .github/workflows/     # CI + release
└── Makefile               # dev tasks
```

## Quickstart

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Run against sample data (no credentials)

```bash
git clone https://github.com/budbuk/budbuk.git
cd budbuk
cargo run -p jira-connector
```

You'll see the four Jira tables (`projects`, `issues`, `users`, `worklogs`) printed
from built-in mock data, plus a caching and a predicate-pushdown demo.

### 3. Run against a real Jira account

Create a `.env` file (it's git-ignored) from the template:

```bash
cp .env.example .env
# then edit .env with your Jira Cloud URL, email, and API token
```

Get an API token at <https://id.atlassian.com/manage-profile/security/api-tokens>.

```bash
cargo run -p jira-connector
```

The CLI switches to **REAL** mode automatically when the three `JIRA_*` variables are set.

## The Jira connector

**Tables:** `projects`, `issues`, `users`, `worklogs`.

**Predicate pushdown.** Filters on pushable columns are translated to JQL so Jira does
the filtering server-side:

| SQL-ish query | Generated JQL |
|---------------|---------------|
| *(no filter)* | `created >= -90d ORDER BY created DESC` |
| `project = 'ENG'` | `project = "ENG" ORDER BY created DESC` |
| `status = 'Open' AND assignee != 'bob'` | `status = "Open" AND assignee != "bob" ORDER BY created DESC` |

Filters on columns Jira can't index are **not** dropped — they're reported back so the
caller (the future FDW layer) re-applies them locally. This keeps results correct.

**Caching.** Every result is cached under a key that includes the account, table, and
full query shape (`https://work.atlassian.net:issues:f[projectEqENG]|s[created-]|p[*]|l[50]`),
so different queries and different accounts never collide. A second identical query is
served from memory — typically ~10,000× faster than the network round-trip.

## Multi-account model

One connector **type** (the code) supports many **instances** (accounts). This mirrors
PostgreSQL's own separation of a foreign-data-wrapper from its foreign servers:

```rust
use jira_connector::{JiraConfig, JiraConnector};

let work = JiraConnector::new(JiraConfig {
    base_url: "https://work.atlassian.net".into(),
    email: "you@work.com".into(),
    api_token: work_token,
    mock: false,
});

let side = JiraConnector::new(JiraConfig {
    base_url: "https://side.atlassian.net".into(),
    email: "you@side.com".into(),
    api_token: side_token,
    mock: false,
});
```

Credentials, rate-limit state, and cache entries are isolated per account.

## Querying from PostgreSQL (the FDW)

The `crates/jira-fdw` crate is a PostgreSQL Foreign Data Wrapper — a native extension
(built with [`pgrx`](https://github.com/pgcentralfoundation/pgrx)) that lets you query
Jira with plain SQL. It's a thin shim: Postgres's scan callbacks are forwarded to the
`JiraConnector` engine, and `WHERE` / `ORDER BY` / `LIMIT` are pushed down to JQL.

> The FDW is **excluded from the main Cargo workspace** — it needs a `pgXX` feature and
> links against PostgreSQL, so it's built and run with `cargo pgrx`, keeping the
> engine's `cargo build/test` and the 100%-coverage gate independent.

Prerequisites: PostgreSQL 14 and the pgrx toolchain (one-time setup):

```bash
cargo install cargo-pgrx --version 0.16.1 --locked
cargo pgrx init --pg14 "$(which pg_config)"
```

Run it (compiles the extension, starts a managed Postgres, opens `psql`):

```bash
cd crates/jira-fdw
cargo pgrx run pg14
```

Then, in `psql`, set up the wrapper and query (full script in
[`crates/jira-fdw/sql/example.sql`](crates/jira-fdw/sql/example.sql)):

```sql
CREATE EXTENSION jira_fdw;
CREATE FOREIGN DATA WRAPPER jira_wrapper
    HANDLER jira_fdw_handler VALIDATOR jira_fdw_validator;
CREATE SERVER jira_account FOREIGN DATA WRAPPER jira_wrapper
    OPTIONS (base_url 'https://your-domain.atlassian.net',
             email 'you@example.com', api_token '…');
CREATE FOREIGN TABLE jira_issues (
    key text, summary text, status text, assignee text, project text, created text
) SERVER jira_account OPTIONS (object 'issues');

SELECT key, status FROM jira_issues WHERE project = 'ENG' LIMIT 5;
--  ^ the WHERE clause is pushed down to Jira as JQL
```

> **Security note:** for this proof of concept, credentials live in the `SERVER`
> options (visible to superusers in the catalogs). A hardened deployment should source
> secrets from a secrets manager. See [Roadmap](#roadmap).

### Any connector in SQL — the generic REST FDW

`crates/rest-fdw` is a *generic* FDW: its `spec` server option carries a serialized
`SourceSpec`, so **any** connector — GitHub, an OpenAPI import, a hand-written spec —
becomes SQL-queryable through one extension. For example, querying GitHub:

```bash
# generate the GitHub spec JSON, then paste it into the CREATE SERVER options
cargo run -p github-connector --example print_spec
```

```sql
CREATE SERVER github FOREIGN DATA WRAPPER rest_wrapper OPTIONS (spec '…SourceSpec JSON…');
CREATE FOREIGN TABLE gh.repos (name text, stars bigint, forks bigint, language text)
    SERVER github OPTIONS (object 'repos');

SELECT name, stars FROM gh.repos ORDER BY stars DESC LIMIT 5;
--  name        | stars
-- -------------+-------
--  Hello-World |  3701   ← live from GitHub, aggregated/sorted by Postgres
```

`WHERE` clauses on filterable columns (e.g. `issues WHERE state = 'open'`) are pushed
down to the API; aggregates, `ORDER BY`, and other filters run in Postgres. Full
example in [`crates/rest-fdw/sql/example.sql`](crates/rest-fdw/sql/example.sql).

## Observability

BudBuk emits structured logs via the [`tracing`](https://docs.rs/tracing) crate. The
demo CLI installs a subscriber; control the level with `RUST_LOG`:

```bash
RUST_LOG="budbuk::cache=debug,budbuk::jira=debug" cargo run -p jira-connector
```

You'll see cache events and HTTP request timing as structured fields:

```
DEBUG budbuk::cache: cache miss, fetching  event="miss" key=https://…:issues:f[]|s[]|p[*]|l[3]
DEBUG budbuk::jira:  url=https://…/rest/api/3/search/jql status=200 elapsed_ms=303
```

Cache counters are available programmatically via `CachedConnector::metrics()`, which
returns a `CacheMetricsSnapshot` (`hits`, `misses`, `stale`, `expired`, `refreshes`) —
ready to feed a `/metrics` endpoint.

## Development

Common tasks are in the [`Makefile`](Makefile):

```bash
make build       # build the workspace
make test        # run all tests
make fmt         # format
make lint        # clippy with warnings denied
make cov         # coverage summary
make cov-check   # fail if any line is uncovered (the 100% gate)
make check       # everything CI runs
```

**Coverage policy.** BudBuk enforces **100% line coverage** in CI (measured with
[`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) and verified from the lcov
report). The only file excluded is the thin binary entrypoint `main.rs`, whose logic is
factored into the tested `cli` module. Sub-line region coverage sits at ~99% (the
residual being error-propagation branches that don't fire on a successful run).

## Roadmap

- [x] Connector SDK: `Connector` trait, neutral types, typed errors
- [x] Jira connector: live REST fetching for projects, issues, users, worklogs
- [x] Pagination (token-based and offset-based)
- [x] Caching: TTL + stale-while-revalidate + per-account namespacing
- [x] Predicate pushdown (`WHERE` / `ORDER BY` / `LIMIT` → JQL)
- [x] Observability: structured logging + tracing + cache metrics
- [x] PostgreSQL FDW layer via [`pgrx`](https://github.com/pgcentralfoundation/pgrx) —
      `SELECT` from Jira in `psql`, with predicate pushdown
- [x] Generic REST FDW (`rest-fdw`) — drives any `SourceSpec` from a server option,
      so GitHub / OpenAPI-imported / hand-written connectors are all SQL-queryable
- [ ] Metrics export (Prometheus / OpenTelemetry)
- [ ] Persistent PostgreSQL-backed cache + incremental sync (shared across queries)
- [ ] Secrets management (secure credential storage; OAuth flows)
- [x] Config-driven **generic REST engine** (`rest-connector`) — any API from a
      declarative `SourceSpec`; auth, pagination, and predicate pushdown built in
- [x] **OpenAPI → `SourceSpec` importer** — auto-generate a connector from an
      OpenAPI document (`SourceSpec::from_openapi`)
- [x] **GitHub** connector (`github-connector`) — repos/issues/gists/orgs as a
      `SourceSpec` over the engine, no bespoke HTTP code
- [ ] More connectors — see the prioritized [connector tracker](CONNECTORS.md)
      (Stripe next, then the GraphQL importer and generic SQL connector)
- [ ] Docker-based local development environment

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) and our
[Code of Conduct](CODE_OF_CONDUCT.md). All PRs must pass CI, including the 100%
line-coverage gate.

## Security

Never commit credentials. To report a vulnerability, see [SECURITY.md](SECURITY.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
