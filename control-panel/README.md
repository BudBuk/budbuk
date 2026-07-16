# BudBuk Control Panel

A web UI + control-plane for BudBuk: **mount connectors**, **sync them into
`shadow.*` tables** on a schedule, and **preview the data** — no SQL required.

This is the MVP of the platform's Phases 2–4 (see the repo [ROADMAP](../ROADMAP.md)):
shadow-table sync, a control-plane API, and a React console.

```
control-panel/
├── server/   # Rust (Axum) control-plane + shadow-sync engine
└── web/      # React + Vite + TypeScript panel
```

## What it does

- **Browse** all 50 built-in connectors.
- **Mount** a source: pick a connector, enter credentials → the server validates
  it and `discover()`s its tables.
- **Sync**: toggle a table on with an interval. The server calls the connector
  engine directly (no FDW), materializes rows into `shadow."<source>__<table>"`
  in PostgreSQL, and re-syncs on schedule. "Sync now" forces a refresh.
- **Preview** the materialized rows.

The server calls the engine directly (`catalog::spec_for` → `RestConnector` →
`discover`/`fetch`), so **sync does not require the FDW**. Verified live: mounting
GitLab and syncing `projects` materializes 1000 rows into `shadow.s1__projects`.

## Architecture

```
React panel ──/api──▶ control-plane (Axum) ──▶ PostgreSQL (shadow.* tables)
                            │  tokio scheduler
                            └─▶ connector engine (catalog → RestConnector) ──▶ SaaS APIs
```

## Run it

**1. A PostgreSQL to sync into** — any instance. (For a quick spin you can point
at a local one.)

**2. Build the web panel:**
```bash
cd control-panel/web
npm install && npm run build      # → dist/
```

**3. Run the control plane** (serves the API *and* the built panel):
```bash
cd control-panel/server
DATABASE_URL='postgres://user@localhost:5432/budbuk' \
  cargo run          # → http://localhost:8080
```
Env: `DATABASE_URL` (required target DB), `PORT` (default 8080), `STATIC_DIR`
(default `../web/dist`). Open <http://localhost:8080>.

For live UI development, run the server and `npm run dev` (Vite proxies `/api`
to `http://localhost:8080`).

## API

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/connectors` | list built-in connector names |
| POST | `/api/sources` | mount `{connector, options}` → discovers tables |
| GET | `/api/sources` | mounted sources + tables + sync status |
| POST | `/api/sources/:id/syncs` | `{table, enabled, intervalSecs}` (also syncs now) |
| POST | `/api/sources/:id/tables/:t/refresh` | sync now |
| GET | `/api/sources/:id/tables/:t/data?limit=` | rows from the shadow table |

## MVP scope & follow-ups

Working now: REST-catalog connectors (49), full-refresh sync, a tokio scheduler,
in-memory source registry, shadow tables, the API, and all three panel screens.

Deliberately deferred (see ROADMAP):
- **Secrets** — credentials are held in memory only; no persistence and no
  secrets backend yet. Don't use real production credentials.
- **Incremental sync** (watermark) — today each sync is a full refresh (capped
  at 1000 rows/table).
- **Persistence** — sources reset on restart.
- **Auth** on the panel, and **GraphQL** sources.
