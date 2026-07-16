# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added (Control panel)

- **Control panel** (`control-panel/`) — a React + Vite single-page app served by an
  Axum control-plane, turning "mount a source and keep its data fresh" into a
  point-and-click flow.
  - **Catalog browser** — all 53 connectors in a searchable, category-filtered card
    grid with bundled brand icons (offline `simple-icons` + monogram fallback).
  - **Spec-driven mount form** — renders each connector's real required/optional
    fields from the catalog, masking secrets.
  - **Credential encryption at rest** — secret fields sealed with **AES-256-GCM**
    (key from `BUDBUK_SECRET_KEY`, or random per-process) before storage.
  - **Credential validation on save** — the server builds the connector, runs
    `discover()`, and does a one-row probe fetch against the live API, so bad
    credentials are rejected up front with a clear error.
  - **Background sync into shadow tables** — a tokio scheduler materializes each
    mounted table into `shadow."<source>__<table>"` (full refresh, capped at 1000
    rows); "sync now" forces a refresh. Sync calls the engine directly, so it does
    **not** require the FDW; GraphQL sources (e.g. Monday.com) are supported.
  - **Analytics view** and **client-side routing** (`/catalog`, `/sources`,
    `/analytics`) with deep-linkable, refresh-safe URLs (SPA fallback returns
    `index.html` at a genuine 200; real assets keep their content-type).
  - JSON API: `/api/connectors`, `/api/sources` (+ `POST` to mount), per-source
    sync toggle / refresh, and shadow-table data preview.
  - Verified live end-to-end against Asana, Freshdesk, Hugging Face (1000 rows),
    Twilio, and Monday.com (GraphQL).

### Added (Connectors)

- **Catalog grown to 53 out-of-the-box connectors.** Beyond the initial set, the
  bundled catalog now covers dev/ITSM/CRM/payments/e-commerce/comms/docs/identity/
  observability/HR sources — each a declarative `SourceSpec` over the shared engine,
  at 100% line coverage.
- **Hugging Face** (`huggingface-connector`) — models, datasets, spaces (Bearer).
- **Monday.com** (`monday-connector`) — boards, users, workspaces over the GraphQL
  engine; the first catalog GraphQL source, wired through the control plane.
- **Granola** (`granola-connector`) — meeting notes over the public API (Bearer).
- **Twilio** paging fix — Twilio deprecated numbered paging (HTTP 400, error 20001);
  switched `messages`/`calls` to single-page fetch.

### Added

- **`IMPORT FOREIGN SCHEMA`** — zero-DDL mounting. `IMPORT FOREIGN SCHEMA x FROM
  SERVER s INTO schema` auto-creates a foreign table for every table a connector
  exposes (from `discover()`), honoring `LIMIT TO` / `EXCEPT`. Implemented in
  `rest-fdw` and `graphql-fdw` over a pure, 100%-covered DDL generator in
  `connector-sdk` (`create_foreign_table_statements`). Verified live against
  GitLab.

### Added (GraphQL)

- **Config-driven GraphQL engine** (`crates/graphql-connector`): the GraphQL twin
  of the REST engine. One engine (`GraphQlConnector`) drives *any* GraphQL API
  from a declarative, serde-serializable `GraphQlSpec` — no per-source code.
  - Each table carries a stored GraphQL document (with variables); the engine
    POSTs `{query, variables}`, surfaces GraphQL `errors`, walks the response to
    the list/connection, and maps nodes to neutral rows.
  - Auth (bearer/basic/API-key header), **Relay cursor pagination** (`first`/
    `after` + `pageInfo`) and plain lists, equality predicate pushdown
    (column → GraphQL variable), and dotted-path field mapping — all from the spec.
  - **Introspection generator** (`GraphQlSpec::from_introspection_json`): the
    analog of the OpenAPI importer. Root `Query` fields returning a Relay
    connection or a list of objects become tables; scalar node fields become
    typed columns and nested objects become a single `Json` column (selected one
    level deep); scalar field arguments become pushdown filter variables; an
    `include` filter focuses generation.
  - Demo CLI queries the public Countries API (`countries.trevorblades.com`) with
    no credentials, using a spec generated from introspection — proving the
    generator feeds the same engine end to end.
- **Generic GraphQL FDW** (`crates/graphql-fdw`): a PostgreSQL extension (pgrx +
  supabase-wrappers) driven by a `spec` server option (a serialized
  `GraphQlSpec`), so any GraphQL source is SQL-queryable through one extension.
  Uses the workspace's rustls-backed `reqwest`, so it shares the FDW segfault
  fix. Verified live from `psql` against the Countries API (projection, sorting,
  nested object as JSON, aggregates). Excluded from the main workspace (built via
  `cargo pgrx`). Example in `crates/graphql-fdw/sql/example.sql`.

### Fixed

- **Segfault when querying an FDW live from PostgreSQL.** HTTP clients used
  `native-tls`, which on macOS links Apple's Security.framework — a fork-unsafe
  library. Inside a `fork()`ed PostgreSQL backend the TLS handshake crashed
  (`SIGSEGV` in CoreAnalytics/`os_log` via `SecTrustCopyPublicKey`), taking down
  the whole backend. Switched `reqwest` to **rustls** (pure-Rust TLS, no Apple
  frameworks) across every connector and both FDWs, and added a `cargo-deny` ban
  so `native-tls` can never be reintroduced. Verified end-to-end: the exact
  query that crashed now returns live data.

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
- **Config-driven REST engine** (`crates/rest-connector`): one engine drives any
  REST API from a declarative, `serde`-serializable `SourceSpec` — no per-source
  code. Individual connectors and OpenAPI-generated connectors share it.
  - `RestConnector` implements `Connector` (plugs into caching, tracing, the FDW).
  - Auth (bearer/basic/API-key header or query), pagination (offset/page/none),
    equality predicate pushdown (column → query param), dotted-path field mapping,
    and JSON→neutral-value conversion — all declared in the spec.
  - Demo CLI runs against the public JSONPlaceholder API with no credentials.
  - **OpenAPI importer** (`SourceSpec::from_openapi` / `from_openapi_json`):
    generates a spec from an OpenAPI 3 document — collection `GET`s become tables,
    `$ref`s are resolved, response types map to columns, and query params become
    equality-pushdown filters. The demo imports a bundled spec and queries live.
  - The REST client now sends a default `User-Agent` (required by some APIs).
  - **Cursor pagination** (`Pagination::Cursor`, e.g. Stripe's `limit` +
    `starting_after` + `has_more`); the importer auto-detects it, and gained an
    `ImportOptions.include` filter to focus a large spec on a few tables.
  - Composed schemas (`anyOf`/`oneOf`/`allOf`, e.g. Stripe's expandable
    "id-or-object" fields) map to their first scalar branch, so ids stay `text`
    rather than quoted JSON.
  - Verified: **Stripe's official OpenAPI (104 tables) imports directly** into a
    working spec — correct columns, pushdown filters, and cursor pagination —
    with no Stripe-specific code, and queried live from `psql` (JOIN across two
    Stripe foreign tables) via the generic REST FDW.
- **GitHub connector** (`crates/github-connector`): repos, issues, gists, and
  orgs exposed as a ~90-line `SourceSpec` over the REST engine — no bespoke HTTP
  code. Works unauthenticated against public data or with a personal access
  token; `WHERE state = '...'` on issues pushes down to the API.
- **Five more out-of-the-box connectors** (built in parallel, each 100% covered):
  **GitLab** (projects/issues/users; page pagination), **Zendesk**
  (tickets/users/organizations; basic auth, pointer row paths), **PagerDuty**
  (incidents/services/users; offset pagination, `Token` header), **Freshdesk**
  (tickets/contacts/companies; basic auth), and **Contentful** (entries/assets/
  content types; nested `sys.*` fields). All registered in the catalog, so they
  mount out-of-the-box. A cross-connector end-to-end test resolves several
  connectors through the catalog, fetches from each against mock servers, and
  combines their rows.
- **Out-of-the-box connectors via a catalog.** Standard connectors now mount with
  just a name + credentials — like Jira, no spec to generate:
  `CREATE SERVER stripe OPTIONS (connector 'stripe', api_key '…')`.
  - `crates/stripe-connector`: bundles a `SourceSpec` (11 core tables, generated
    from Stripe's official OpenAPI) so Stripe needs only an API key.
  - `crates/catalog`: maps a connector name + options to a `SourceSpec`
    (`stripe`, `github` built-in; `openapi` imports a supplied doc). Adding a
    standard connector is "bundle a spec, add one match arm".
  - `rest-fdw` reads a `connector` server option and asks the catalog; the raw
    `spec` option remains for fully custom sources.
- **Generic REST FDW** (`crates/rest-fdw`): a PostgreSQL extension (pgrx +
  supabase-wrappers) driven by a `spec` server option (a serialized `SourceSpec`),
  so any connector — GitHub, an OpenAPI import, a hand-written spec — is
  SQL-queryable through one extension. Verified by querying GitHub live from
  `psql` (repos/issues/gists) with pushdown, aggregates, and sorting. Excluded
  from the main workspace (built via `cargo pgrx`). Example in
  `crates/rest-fdw/sql/example.sql`.
- Project infrastructure: dual MIT/Apache-2.0 license, README and community docs,
  GitHub Actions CI (fmt, clippy, tests on stable + MSRV, 100% line-coverage gate,
  `cargo-deny`, docs), release workflow, Dependabot, and a `Makefile` of dev tasks.

[Unreleased]: https://github.com/budbuk/budbuk/commits/main
