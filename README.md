<p align="center">
  <img src="assets/logo.png" alt="BudBuk logo" width="132" height="132">
</p>

<h1 align="center">BudBuk</h1>

<p align="center">
  <strong>A high-performance, PostgreSQL-native data integration platform in Rust.</strong><br>
  Query <strong>50+ SaaS sources</strong> — Jira, GitHub, Stripe, Slack, Salesforce-style CRMs, and more — with plain SQL. Fast, cached, and safe.
</p>

<p align="center">
  <a href="https://github.com/budbuk/budbuk/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/budbuk/budbuk/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="Coverage" src="https://img.shields.io/badge/coverage-100%25%20lines-brightgreen">
  <img alt="License" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue">
  <img alt="MSRV" src="https://img.shields.io/badge/rustc-1.88%2B-orange">
</p>

<p align="center">
  <a href="#connectors"><b>Connectors</b></a> ·
  <a href="ROADMAP.md"><b>Roadmap</b></a> ·
  <a href="docs/configuration.md"><b>Configuration</b></a> ·
  <a href="CONNECTORS.md"><b>Connector tracker</b></a> ·
  <a href="CONTRIBUTING.md"><b>Contributing</b></a>
</p>

---

> **Status: working proof of concept.** BudBuk ships **50 out-of-the-box connectors**
> plus generic **REST/OpenAPI** and **GraphQL** engines — all queryable from PostgreSQL
> via a `pgrx` Foreign Data Wrapper. The engine (schema discovery, live fetching,
> pagination, caching, predicate pushdown, observability) is proven live against real
> Jira, GitHub, Stripe, GitLab, and GraphQL endpoints. See
> [Connectors](#connectors) and [Querying from PostgreSQL](#querying-from-postgresql-the-fdw).

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

## Connectors

**50 sources ship out-of-the-box.** Mount any of them with just a connector name and
credentials — exactly like Jira: `CREATE SERVER x OPTIONS (connector 'stripe', api_key '…')`.
Each is a declarative `SourceSpec` over the shared engine (no bespoke HTTP code), at 100%
line coverage. The long tail is covered by generic **REST/OpenAPI** and **GraphQL**
connectors — bring your own spec or OpenAPI/introspection document.

| Category | Connectors |
|----------|------------|
| **Dev & issues** | GitHub · GitLab · Bitbucket · Jira · Jira Service Management · Sentry |
| **Support & ITSM** | Zendesk · Freshdesk · Intercom · ServiceNow · PagerDuty · Opsgenie |
| **CRM & marketing** | HubSpot · Pipedrive · Zoho CRM · ActiveCampaign · Mailchimp · Klaviyo |
| **Payments & billing** | Stripe · PayPal · Square · Chargebee · Recurly |
| **E-commerce** | Shopify · WooCommerce · BigCommerce |
| **Comms & meetings** | Slack · Zoom · Twilio · Calendly |
| **Work, docs & CMS** | Asana · Smartsheet · Notion · Confluence · Contentful |
| **Forms & email** | Typeform · SurveyMonkey · SendGrid |
| **Identity & files** | Okta · Auth0 · Box · Google Drive · Microsoft Graph |
| **Observability** | Datadog · Grafana |
| **HR & recruiting** | Greenhouse · Lever |
| **Finance & other** | Xero · DocuSign · Google Calendar |
| **Meta (bring your own)** | Generic REST / OpenAPI · Generic GraphQL |

📖 **See the full [connector tracker →](CONNECTORS.md)** for status, auth types, per-source
notes, and the roadmap of what's next.

## Features

| Area | What you get |
|------|--------------|
| **50 connectors** | Bundled `SourceSpec`s mount out-of-the-box via a **catalog** — just a name + credentials. |
| **REST + GraphQL** | One config-driven REST engine *and* a GraphQL engine; generate specs from OpenAPI or GraphQL introspection. |
| **Schema discovery** | Each connector exposes typed tables (columns + PostgreSQL-ish types). |
| **Live fetching** | Async HTTP (`reqwest` + **rustls**) with typed JSON parsing (`serde`). |
| **Pagination** | Offset, page-number, cursor (Stripe-style), and Relay GraphQL connections. |
| **Caching** | In-memory TTL cache with **stale-while-revalidate** and a thundering-herd guard. |
| **Predicate pushdown** | Translates `WHERE` / `ORDER BY` / `LIMIT` into the source's query params or query language (JQL for Jira). |
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
│   ├── graphql-connector/ # Config-driven GraphQL engine + introspection generator
│   │   └── src/                # GraphQlSpec, GraphQlConnector, introspect.rs, cli.rs
│   ├── <source>-connector/# 45+ bundled connectors, each a SourceSpec (no HTTP code):
│   │   └── src/lib.rs          #   github, stripe, slack, hubspot, notion, datadog, …
│   ├── catalog/           # Maps a connector name → bundled SourceSpec (out-of-the-box)
│   │   ├── src/lib.rs          # spec_for("stripe", opts) → SourceSpec
│   │   └── tests/              # cross-connector end-to-end tests
│   ├── jira-fdw/          # PostgreSQL FDW for Jira (pgrx; excluded from workspace)
│   ├── rest-fdw/          # Generic REST FDW: any SourceSpec/catalog name → SQL (pgrx)
│   └── graphql-fdw/       # Generic GraphQL FDW: any GraphQlSpec → SQL (pgrx)
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

### Any connector in SQL — one FDW, built-in connectors

`crates/rest-fdw` is a single generic extension that serves every connector. Standard
connectors are **out-of-the-box**: their specs are bundled in the code (a *catalog*), so
you mount them with just a name + credentials — exactly like Jira, no spec to generate:

```sql
CREATE EXTENSION rest_fdw;
CREATE FOREIGN DATA WRAPPER budbuk HANDLER rest_fdw_handler VALIDATOR rest_fdw_validator;

-- Built-in connectors: pick a name, give only credentials/config
CREATE SERVER stripe OPTIONS (connector 'stripe', api_key 'sk_live_…');
CREATE SERVER gh     OPTIONS (connector 'github', owner 'acme', repo 'app', token 'ghp_…');

-- The long tail: bring your own OpenAPI doc (or a raw SourceSpec)
CREATE SERVER myapi  OPTIONS (connector 'openapi', spec '…openapi json…', token '…');

CREATE FOREIGN TABLE stripe.charges (id text, amount bigint, status text, customer text)
    SERVER stripe OPTIONS (object 'charges');
SELECT sum(amount)/100.0 AS revenue FROM stripe.charges WHERE status = 'succeeded';
```

`WHERE` clauses on filterable columns push down to the API; aggregates, `ORDER BY`, joins,
and other filters run in Postgres. The connector **catalog** (`crates/catalog`) maps each
name to a bundled `SourceSpec` — adding a standard connector is "bundle a spec, add one
line".

**GraphQL sources** work the same way through `crates/graphql-fdw`: mount a serialized
`GraphQlSpec` (hand-written or generated from schema introspection) and query it in SQL,
with Relay cursor pagination handled for you. Example in
[`crates/graphql-fdw/sql/example.sql`](crates/graphql-fdw/sql/example.sql).

📖 **Full setup reference — every connector, plus multiple accounts (e.g. two Jira sites)
and the isolation/security model — is in [docs/configuration.md](docs/configuration.md).**
A runnable example is in [`crates/rest-fdw/sql/example.sql`](crates/rest-fdw/sql/example.sql).

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

> **Platform vision** — zero-DDL setup (`IMPORT FOREIGN SCHEMA`), background data sync
> (shadow tables), a React management console, and an agent (MCP) layer — is mapped out in
> **[ROADMAP.md](ROADMAP.md)**, with verified feasibility notes. The checklist below tracks
> the engine.

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
      OpenAPI document (`SourceSpec::from_openapi`); imports **Stripe's official
      104-table spec** directly, with cursor pagination auto-detected
- [x] **50 out-of-the-box connectors** via a bundled **catalog** — GitHub, Stripe,
      Slack, HubSpot, Notion, Datadog, and 44 more mount with just a name + credentials
      (see [Connectors](#connectors) / the [tracker](CONNECTORS.md))
- [x] **GraphQL** connector (`graphql-connector`) — config-driven engine, Relay cursor
      pagination, and an **introspection → spec generator**, plus a generic GraphQL FDW
- [x] `AuthSpec::Headers` for multi-header APIs (Notion, Datadog, Klaviyo, Xero)
- [ ] Generic **SQL database** connector (Postgres/MySQL/etc. as a source)
- [ ] More connectors — see the prioritized [connector tracker](CONNECTORS.md)
- [ ] Docker-based local development environment

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) and our
[Code of Conduct](CODE_OF_CONDUCT.md). All PRs must pass CI, including the 100%
line-coverage gate.

## Contributors

Thanks to everyone building BudBuk. This table grows with every merged PR.

<table>
  <tr>
    <td align="center" valign="top" width="150">
      <a href="https://github.com/jigardafda">
        <img src="https://github.com/jigardafda.png" width="80" height="80" alt="Jigar Dafda"><br>
        <sub><b>Jigar Dafda</b></sub>
      </a><br>
      <sub>Creator &amp; maintainer</sub>
    </td>
    <!-- new contributors are added here -->
  </tr>
</table>

See the full list on the [contributors graph](https://github.com/BudBuk/budbuk/graphs/contributors).
Want to be on it? Pick up a [good first issue](https://github.com/BudBuk/budbuk/labels/good%20first%20issue)
or a connector from the [tracker](CONNECTORS.md).

## Security

Never commit credentials. To report a vulnerability, see [SECURITY.md](SECURITY.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
