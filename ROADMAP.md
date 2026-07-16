# BudBuk Platform Roadmap

BudBuk today is a **PostgreSQL-native connector engine**: 50 out-of-the-box sources
(plus generic REST/OpenAPI and GraphQL), queryable as foreign tables via a `pgrx` FDW,
with predicate pushdown, an in-memory cache, and 100% test coverage.

This document is the roadmap for turning that engine into a **full data-integration
platform**: zero-DDL setup, background data sync, a management UI, and an agent layer —
all Postgres-native.

> This is a living document. For the shipped/near-term connector list, see
> [CONNECTORS.md](CONNECTORS.md). For per-feature design specs, see
> [`docs/superpowers/specs/`](docs/superpowers/specs/).

---

## Where we are

| Capability | Status |
|---|---|
| Config-driven REST engine + OpenAPI importer | ✅ |
| GraphQL engine + introspection generator + FDW | ✅ |
| 50 out-of-the-box connectors via a catalog | ✅ |
| Generic REST FDW (`connector`/`spec` options) | ✅ |
| Predicate pushdown (`WHERE`/`ORDER BY`/`LIMIT`) | ✅ |
| In-memory cache (TTL + stale-while-revalidate) | ✅ |
| rustls TLS (fork-safe inside the backend) | ✅ |
| Live-verified in `psql` | ⚠️ ~5 of 50 (Jira, GitHub, Stripe, GitLab, GraphQL) |
| Mounting foreign tables | ✅ `IMPORT FOREIGN SCHEMA` (auto) or manual |
| Data materialization / sync | ❌ (live-only) |
| Setup UI | ❌ |
| Agent access (MCP) | ❌ |

---

## Feasibility research (verified against the installed toolchain)

Everything the platform needs is exposed by `supabase-wrappers` 0.1.28 and `pgrx` 0.16.1:

- **`IMPORT FOREIGN SCHEMA` — supported.**
  `ForeignDataWrapper::import_foreign_schema(&mut self, stmt: ImportForeignSchemaStmt)
  -> Result<Vec<String>, E>` returns a list of `CREATE FOREIGN TABLE` statements the core
  server executes. `ImportForeignSchemaStmt` carries `server_name`, `remote_schema`,
  `local_schema`, `list_type` (`ALL` / `LIMIT TO` / `EXCEPT`), `table_list`, and `options`
  — so per-table filtering is built in.
- **Write-back — supported.** The trait has `begin_modify` / `insert` / `update` /
  `delete` / `end_modify` (default no-ops). This is the basis for **action connectors**
  (create a Jira ticket, update a HubSpot deal) and for agent "act" tools.
- **Aggregate pushdown — supported.** `supported_aggregates`, `supports_group_by`,
  `get_aggregate_rel_size` allow pushing `COUNT`/`SUM`/`GROUP BY` to the source later.
- **Background sync — supported.** `pgrx::bgworkers::BackgroundWorker` +
  `BackgroundWorkerBuilder` (`enable_spi_access()`, `load_dynamic()`) let a worker run
  inside Postgres and write via SPI. Alternatives: the `pg_cron` extension, or an external
  Rust service (also the sidecar option below).
- **Agent layer — supported.** The `Connector` trait's `discover()`/`fetch()` map 1:1 to
  MCP tool needs; the official Rust MCP SDK (`rmcp`) provides the JSON-RPC/transport layer.

---

## Phase 1 — Zero-DDL setup: `IMPORT FOREIGN SCHEMA` ✅ DONE

**Goal:** mount every table of a connector with one command, no hand-written DDL.

```sql
CREATE SERVER stripe FOREIGN DATA WRAPPER budbuk
    OPTIONS (connector 'stripe', api_key 'sk_live_…');
IMPORT FOREIGN SCHEMA stripe FROM SERVER stripe INTO stripe;
--  ↳ auto-creates stripe.charges, stripe.customers, … with typed columns
--  LIMIT TO (charges, customers)  /  EXCEPT (events)  supported
```

**How:** implement `import_foreign_schema` in `rest-fdw` and `graphql-fdw`:
1. Build the connector from the server options (reuse `catalog::spec_for` / the `spec`).
2. Call `discover()` → `Vec<TableSchema>`.
3. Honor `list_type` + `table_list` (`ALL` / `LIMIT TO` / `EXCEPT`).
4. Emit a `CREATE FOREIGN TABLE local_schema.<t> (<cols>) SERVER <s> OPTIONS (object '<t>')`
   per table.

**Type mapping** (`connector_sdk::DataType` → Postgres): `Text→text`, `Integer→bigint`,
`Float→double precision`, `Bool→boolean`, `Json→jsonb`, `Timestamp→timestamptz`.
*Caveat to resolve:* the engine currently materializes `Json`/`Timestamp` values as strings
(`Cell::String`); Phase 1 must either emit `text`/`text` for those or teach `value_to_cell`
to produce true `jsonb`/`timestamptz` cells. Start with the safe mapping, upgrade after.

**Delivered:** `import_foreign_schema` in `rest-fdw` + `graphql-fdw`; a pure `discover → DDL`
helper in `connector-sdk` (`create_foreign_table_statements`, 100% covered); honors
`ALL`/`LIMIT TO`/`EXCEPT`. Verified live: `IMPORT FOREIGN SCHEMA … FROM SERVER gl INTO gl`
auto-created GitLab's tables and queried them, no crash.

---

## Phase 2 — Shadow tables + interval sync

**Goal:** optionally keep a **materialized local copy** of any foreign table, refreshed on a
schedule — turning BudBuk into a lightweight, Postgres-native ELT engine (Airbyte/Fivetran-
style) that is *also* queryable live.

```
stripe.charges           ← live foreign table (fresh, pushdown, API round-trip)
stripe._charges_shadow   ← real table, UPSERTed every N minutes by a sync worker
stripe.charges (view)    ← optional: serve shadow if fresh, else live (SWR per table)
```

**Why:** fast local queries (indexes, unrestricted joins, no pushdown limits), resilience
when the API is down/rate-limited, and **history** (snapshots the SaaS API won't give you).
This is the durable evolution of the in-memory TTL+SWR cache we already ship.

**Design:**
- **Sync worker** — a `pgrx` `BackgroundWorker` (or `pg_cron`, or the external control-plane
  service) that periodically calls `fetch()` and `UPSERT`s into the shadow table.
- **Incremental sync** — a per-table watermark (`updated_since` cursor) so only changed rows
  are pulled; needs an incremental filter on the connector (most APIs support one).
- **Change tracking / CDC** — optional history table capturing insert/update/delete over
  time.
- **Sync config** — a `budbuk.sync` catalog table (connector, table, interval, mode, last
  run, row counts, status).

**Open questions:** watermark column per connector; full-refresh vs incremental fallback;
conflict/tombstone handling for deletes; back-pressure vs API rate limits.

**Effort: medium–large.** This is the flagship differentiator.

---

## Phase 3 — Control-plane API

**Goal:** a programmatic layer so setup/secrets/sync are driven by an API, not raw SQL.

```
React console ──REST/gRPC──▶ control-plane (Rust/Axum) ──▶ Postgres + BudBuk FDW
                                   ├─ runs CREATE SERVER + IMPORT FOREIGN SCHEMA
                                   ├─ stores credentials in a secrets manager
                                   └─ schedules & monitors syncs (Phase 2)
```

**Responsibilities:** mount/unmount connectors; **credential storage** (not plaintext
`SERVER` options — env/Vault/cloud secrets); sync scheduling + status; health/metrics
aggregation; multi-account management. Reuses `catalog`, `connector-sdk`, and the engines
directly.

**Effort: medium.**

---

## Phase 4 — React management console

**Goal:** point-and-click setup — nobody writes SQL to onboard a source.

- **Catalog browser** — all 50 connectors, their tables/columns.
- **One-click mount** — pick connector → enter credentials → runs `CREATE SERVER` +
  `IMPORT FOREIGN SCHEMA` under the hood.
- **Schema explorer + query console** — preview tables, sample rows, run SQL (pgAdmin-lite,
  scoped to BudBuk).
- **Sync configuration** — toggle shadow tables, set intervals, watch progress (Phase 2).
- **Observability** — connector status, last sync, cache hit-rate, errors, rate-limit state.
- **Multi-account** — manage e.g. two Jira sites side by side.

Stack: React + TypeScript, talking to the Phase 3 control-plane. **Effort: large.**

---

## Phase 5 — Agent layer (MCP)

**Goal:** let any AI agent (Claude, etc.) query and act on all sources — Postgres-native.

- **MCP server** (`budbuk-mcp`) exposing:
  - `list_foreign_tables()` / `describe(table)` — schema context from `discover()`.
  - `run_sql(sql)` — **read-only, guarded** (single `SELECT`, statement timeout, row cap,
    restricted role) against the FDW-mounted Postgres → agents get **cross-source JOINs and
    aggregates**, the thing the raw engine can't do alone.
  - *(later)* `act(connector, action, args)` — write-back via the FDW `insert`/`update`/
    `delete` hooks.
- **Natural-language → SQL**, grounded in the discovered schemas.
- **pgvector + RAG** — embed connector data in Postgres for semantic search the agent can
  retrieve *and* join.

Postgres stays the reasoning engine; the agent just drives it. **Effort: medium** (builds on
Phases 1–3). See the earlier design discussion for the direct-engine vs Postgres-backed
trade-off — the platform commits to **Postgres-backed**.

---

## Cross-cutting / foundational tracks

- **Robustness — in-process runtime vs sidecar.** The FDW runs the async engine inside the
  Postgres backend. rustls fixed the TLS segfault, but large multi-page fetches and many
  concurrent connectors are unproven. The durable fix is the **sidecar**: move the engine to
  a separate process; the FDW becomes a thin synchronous IPC shim, so no connector can crash
  the backend. Prerequisite for heavy production use.
- **Completeness — live-verify all 50.** Only ~5 are proven end-to-end in `psql`; the rest
  are mock-verified specs. Mount each against a real API, fix mismatches, prove cross-source
  joins across many.
- **Secrets management.** Replace plaintext `SERVER` options with a secrets backend (feeds
  Phase 3/4).
- **Generic SQL-database connector.** Unlocks Postgres/MySQL/warehouses as sources — a whole
  new source class beyond REST/GraphQL.

---

## Suggested sequence

1. **Phase 1 — `IMPORT FOREIGN SCHEMA`** (small, foundational; everything else builds on it).
2. **Phase 2 — shadow tables + sync** (the differentiator).
3. **Phase 3 — control-plane API** (wraps 1 & 2).
4. **Phase 4 — React console** (UI on the control-plane).
5. **Phase 5 — MCP / agent layer** (on top of the platform).

Foundational tracks (robustness/sidecar, completeness, secrets) run alongside as needed.
Each phase is independently shippable and built at the same 100%-coverage bar as the engine.
