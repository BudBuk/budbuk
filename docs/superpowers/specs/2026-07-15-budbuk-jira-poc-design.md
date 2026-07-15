# budbuk — Design (Jira PoC slice)

**Date:** 2026-07-15
**Status:** Approved (lean, learn-by-doing path)

## What we're building

A PostgreSQL-native data integration platform in Rust. Users query external SaaS
data (Jira, GitHub, Slack, …) directly with SQL, as PostgreSQL Foreign Data
Wrappers (FDWs), with intelligent caching.

This spec covers the **first slice**: the reusable connector framework + a Jira
connector, built engine-first and later wrapped as a Postgres FDW.

## Target architecture (hybrid, 3 layers)

1. **FDW shim** — thin in-process PostgreSQL extension (via `pgrx`). Implements
   FDW callbacks, extracts pushdown (quals, projection, sort, limit). No network
   I/O itself; forwards scan requests to the engine.
2. **Connector Engine** — standalone async Rust service/library. Owns the
   connector SDK, async fetching, pooling, rate limiting, retries, pagination,
   auth/secrets, and the cache. Slowness is contained here, never in a PG backend.
3. **Two-tier cache** — in-memory hot cache (moka) backed by durable Postgres
   tables (persistent cache / materialized store). Enables TTL, stale-while-
   revalidate, incremental sync, manual refresh.

Rationale: combines the live-FDW feel of an in-process FDW, the isolation of a
sidecar, and the speed/robustness of materialization — while keeping connector
code in a normal async Rust binary that's easy to build, test, and reuse.

## Connector type vs. instance (multi-account)

- **Type** = the connector *code* (one per source, e.g. Jira), a Rust type
  implementing the `Connector` trait.
- **Instance/account** = a PostgreSQL **foreign server** + **user mapping**
  (URL + credentials). Many instances per type (e.g. two Jira accounts).
- Connector code is account-agnostic: it receives an account's config each call.
- **Cache keys are namespaced per account.** Credentials and rate-limit state are
  per-account and isolated.

## Learning-first build path

- **Step 0 — Install Rust.** ✅ Done (rustc/cargo 1.97).
- **Step 1 — Jira connector as a plain Rust CLI.** `Connector` trait +
  `JiraConnector` struct; fetch projects/issues/users/worklogs over HTTP
  (`reqwest`), parse JSON (`serde`), async (`tokio`), `Result` error handling.
  Mock mode so it runs without credentials; real Jira when ready.
- **Step 2 — Production-shaped.** Pagination, filtering/JQL pushdown, caching with
  TTL, rate-limit handling, observability (structured logs + tracing).
- **Step 3 — Postgres FDW.** Wrap the engine with `pgrx`; foreign servers + user
  mappings realize the two-account model; `SELECT * FROM jira_work.issues`.

## Repository structure (initial)

```
budbuk/
  Cargo.toml            # workspace
  crates/
    connector-sdk/      # reusable framework: Connector trait, errors, types
    jira-connector/     # Jira impl + CLI to run it
  docs/
```

## Ecosystem choices

- **Async:** tokio. **HTTP:** reqwest. **JSON:** serde / serde_json.
- **Errors:** thiserror (library errors) + anyhow (app/CLI). **Cache:** moka.
- **Observability:** tracing + tracing-subscriber.
- **Postgres FDW (Step 3):** pgrx.

## Out of scope (roadmap)

Additional connectors (GitHub, Slack, Salesforce, Sheets, generic REST/DB),
OAuth flows, full sidecar/IPC transport, secrets manager integration, CI/CD.
Each future connector reuses the SDK by implementing `Connector`.
