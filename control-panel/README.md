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

- **Browse** all 53 built-in connectors in a searchable, category-filtered card
  grid (brand icons, category badges, descriptions).
- **Mount** a source: the form renders the connector's real fields (from the
  catalog), masking secrets. On submit the server **validates the credentials**
  by probing the API (a 1-row fetch), so bad creds are caught up front. Secret
  values are **AES-256-GCM encrypted** before storage.
- **Sync**: toggle a table on with an interval. The server calls the connector
  engine directly (no FDW), materializes rows into `shadow."<source>__<table>"`
  in PostgreSQL, and re-syncs on schedule. "Sync now" forces a refresh.
- **Preview** the materialized rows.

The server calls the engine directly (`catalog::spec_for` → `RestConnector`, or a
`GraphQlConnector` for GraphQL sources → `discover`/`fetch`), so **sync does not
require the FDW**. Verified live end-to-end against Asana, Freshdesk, Hugging Face
(1000 rows), Twilio, and Monday.com (GraphQL).

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

Working now: REST-catalog connectors (52) **and GraphQL sources** (e.g. Monday.com),
credential encryption + live validation, full-refresh sync, a tokio scheduler,
in-memory source registry, shadow tables, the API, and all three panel screens
(catalog, sources, analytics) with client-side routing.

Deliberately deferred (see ROADMAP):
- **Secrets** — secret values are AES-256-GCM encrypted at rest (key from
  `BUDBUK_SECRET_KEY`, or random per-process), but the source registry itself is
  still in-memory and not persisted. A dedicated secrets backend + persistence
  is the next step; don't rely on it for production credentials yet.
- **Incremental sync** (watermark) — today each sync is a full refresh (capped
  at 1000 rows/table).
- **Persistence** — sources reset on restart.
- **Auth** on the panel.
