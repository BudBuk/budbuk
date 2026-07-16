# GraphQL Connector + FDW — Design

**Date:** 2026-07-16
**Status:** Approved (pending spec review)

## Goal

Add first-class GraphQL support to BudBuk, structured exactly like the existing
REST support: a declarative spec drives a config-driven engine, a generator
produces specs from a GraphQL schema (the analog of the OpenAPI importer), a
demo CLI exercises it, and a PostgreSQL FDW makes it queryable from `psql`.

"Same as REST" is the north star: every REST artifact gets a GraphQL twin.

| REST | GraphQL |
|------|---------|
| `rest-connector` crate | `graphql-connector` crate |
| `SourceSpec` (`spec.rs`) | `GraphQlSpec` (`spec.rs`) |
| `RestConnector` (`connector.rs`) | `GraphQlConnector` (`connector.rs`) |
| OpenAPI importer (`openapi.rs`) | Introspection generator (`introspect.rs`) |
| `rest-cli` (`cli.rs`/`main.rs`) | `graphql-cli` (`cli.rs`/`main.rs`) |
| `rest-fdw` (pgrx extension) | `graphql-fdw` (pgrx extension) |

## Non-goals (v1)

- Catalog wiring (`connector 'x'` name resolution). The catalog currently
  returns a `SourceSpec` tied to `RestConnector`; supporting a second connector
  type is a separate refactor. GraphQL mounts via a serialized `spec` option,
  exactly like `rest-fdw`'s raw-spec path.
- Mutations/subscriptions — read-only queries only.
- Offset pagination — `None` + Relay cursor connections cover the vast majority;
  offset is a cheap later addition (YAGNI).
- Hoisting shared auth/JSON primitives into `connector-sdk`. `graphql-connector`
  keeps small self-contained copies so this work does not touch `rest-connector`
  and its 100%-coverage gate. Hoisting is a flagged follow-up.

## The spec (`GraphQlSpec`)

GraphQL is a single endpoint reached by `POST {query, variables}`. A "table" is
therefore a stored GraphQL **document with variables**; the engine injects
pagination and filter values at query time.

```rust
struct GraphQlSpec {
    name: String,
    endpoint: String,          // single POST URL (analog of base_url)
    auth: AuthSpec,            // None | Bearer | Basic | ApiKeyHeader (self-contained copy)
    tables: Vec<GraphQlTable>,
}

struct GraphQlTable {
    name: String,
    query: String,             // e.g. query($first:Int,$after:String,$state:String){ ... }
    data_pointer: String,      // JSON pointer *under* `data` to the connection/list
    shape: NodeShape,          // Connection | List
    columns: Vec<ColumnSpec>,  // dotted path into a node -> typed column
    pagination: GraphQlPagination, // None | Relay { first_var, after_var, page_size }
    filters: Vec<FilterVar>,   // column -> GraphQL variable name (equality pushdown)
}

enum NodeShape { Connection, List }   // Connection = edges[].node + pageInfo (Relay)
enum GraphQlPagination {
    None,
    Relay { first_var: String, after_var: String, page_size: usize },
}
struct ColumnSpec { name: String, field: String, data_type: DataType }
struct FilterVar  { column: String, variable: String }
```

Everything derives `serde` so specs load from JSON and the generator can emit them.

## The engine (`GraphQlConnector`)

Implements the `connector-sdk` `Connector` trait (`name`, `discover`, `fetch`),
so it plugs into the same caching, tracing, and FDW machinery as REST.

`fetch(table, query)`:
1. Build the variables map: pagination vars (`first`/`after`) + pushed-down
   equality filters (column → declared variable).
2. `POST endpoint` with `{query, variables}`; apply auth to the request.
3. Parse the response. GraphQL returns `{ "data": ..., "errors": [...] }`.
   Non-empty `errors` → `ConnectorError::Other` (join messages).
4. Locate the payload at `/data` + `data_pointer`.
   - `NodeShape::Connection`: iterate `/edges/*/node`; read `/pageInfo/hasNextPage`
     and `/pageInfo/endCursor`; loop with `after = endCursor` until no next page or
     the caller's `LIMIT` is met.
   - `NodeShape::List`: take the array directly (single request).
5. Map each node's columns via dotted field paths into neutral `Row`s; truncate to `LIMIT`.

Relay's `edges/node/pageInfo` layout is the standardized connection shape and is
assumed, so it is not re-declared per table — only the connection's location
(`data_pointer`) and page size vary.

## The generator (`introspect.rs`)

Input: the JSON result of the standard GraphQL introspection query (`__schema`).
`GraphQlSpec::from_introspection_json(doc, ImportOptions)` produces a spec.

Heuristic (chosen: "connections + object lists", applied to **root `Query` fields**):
- Unwrap `NON_NULL`/`LIST` wrappers on each root field's type.
- If it resolves to an OBJECT that is a **Relay connection** (has an `edges`
  field whose node is an object) → table, `NodeShape::Connection`.
- Else if the field returns a **list of objects** → table, `NodeShape::List`.
- Else (scalar, or single object) → skip.
- Node columns: scalar fields (`String`/`Int`/`Float`/`Boolean`/`ID`/enums) →
  typed `ColumnSpec`; object/list fields → a single `Json` column (safe default).
- Required scalar arguments on the field → `FilterVar`s (surfaced as pushdown
  variables; the user supplies them via `WHERE`).
- The generated `query` string is assembled from the chosen columns plus
  `first`/`after` variables for connections.
- `ImportOptions { auth, endpoint, include: Option<Vec<String>> }` — `include`
  filters to a subset of tables (mirrors the OpenAPI importer).

Honest scope: works cleanly when the schema exposes list/connection fields at the
**root**. Connections that only exist nested under required-argument parents
(e.g. GitHub's `repository(owner,name){ issues }`) are better hand-written —
exactly as REST relies on hand-written specs beyond the clean OpenAPI cases.

## The CLI (`graphql-cli`)

Demo binary: introspect a public endpoint (or load a bundled introspection
sample), print the generated tables, then run a live query and print rows.
Mirrors `rest-connector`'s CLI/examples.

## The FDW (`graphql-fdw`)

A `pgrx` + `supabase-wrappers` extension, **excluded from the Cargo workspace**
(like `rest-fdw`/`jira-fdw`), so `cargo test` and the coverage gate stay on the
engine crates.

- `GraphQlFdw { connector: GraphQlConnector, schema_cols, rows, cursor, tgt_cols }`.
- `new(server)`: read the `spec` server option (serialized `GraphQlSpec`),
  deserialize, build the connector.
- `begin_scan`: read the `object` table option; `discover()` for the schema;
  `build_query` from quals/columns/sorts/limit; `fetch()` via `create_async_runtime().block_on(...)`.
- `iter_scan`/`re_scan`/`end_scan`/`get_rel_size`: identical pattern to `rest-fdw`.
- Inherits the workspace `reqwest` **rustls** backend, so it is free of the
  native-tls fork-unsafe segfault fixed on 2026-07-16.
- `sql/example.sql` demonstrates mounting a GraphQL source and querying it.

## Error handling

- GraphQL `errors` array → `ConnectorError::Other` with joined messages.
- HTTP non-2xx → `ConnectorError::Auth` (401/403) or `::Other`.
- Network/parse failures → `ConnectorError::Network` / `::Parse`.
- Missing `data_pointer` target or wrong shape → `ConnectorError::Parse` naming the table.
- FDW surfaces these as `ERRCODE_FDW_ERROR`, never a crash.

## Testing

- **Unit + wiremock (in-workspace, 100% line coverage):**
  - Engine: Relay multi-page pagination, `List` single-fetch, `errors` handling,
    each auth variant, equality pushdown → variables, `LIMIT` truncation,
    column mapping incl. nested → `Json`, missing/mismatched fields → `Null`.
  - Generator: connection detection, list detection, scalar-vs-`Json` columns,
    required-arg → `FilterVar`, `include` filter, type unwrapping.
  - Spec: JSON round-trip and every enum variant.
- **Live smoke (network-gated):** introspect + query `countries.trevorblades.com`
  (public, no auth) — proves real-endpoint wiring without secrets.
- **FDW:** validated live from `psql` via `cargo pgrx run`, mirroring `rest-fdw`.

## Rollout

1. `graphql-connector`: `spec.rs` → `connector.rs` (engine) → `introspect.rs`
   (generator) → `cli.rs`/`main.rs`, each with tests, kept at 100% coverage.
2. Wire into the workspace (`Cargo.toml` members + deps).
3. `graphql-fdw` crate (excluded), `sql/example.sql`.
4. Live smoke test + live `psql` validation.
5. `CHANGELOG.md` entry.
