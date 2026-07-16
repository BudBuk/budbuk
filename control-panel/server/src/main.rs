//! BudBuk control plane.
//!
//! A small Axum service that mounts connectors (via the `catalog`), syncs them
//! into `shadow.*` tables in PostgreSQL on a schedule, serves a JSON API, and
//! serves the React control panel. It calls the connector engine directly, so
//! sync does not require the FDW.
//!
//! MVP scope: in-memory source registry (credentials are held in memory, not
//! persisted — a secrets backend is a documented follow-up) and full-refresh
//! sync (drop + reload).

mod crypto;
mod sql;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query as AxumQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use deadpool_postgres::{Config as PgConfig, Pool, Runtime};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::RwLock;
use tokio_postgres::NoTls;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use catalog::spec_for;
use connector_sdk::{Connector, Query, TableSchema};
use graphql_connector::GraphQlConnector;
use monday_connector::{monday_spec, MondayConfig};
use rest_connector::RestConnector;

/// GraphQL connectors (built on the GraphQL engine, not the REST catalog).
const GRAPHQL_CONNECTORS: &[&str] = &["monday"];

/// How many rows a sync pulls per table (full-refresh MVP).
const SYNC_LIMIT: usize = 1000;

// ─────────────────────────────── state ───────────────────────────────

#[derive(Clone)]
struct AppState {
    pool: Pool,
    sources: Arc<RwLock<HashMap<String, Source>>>,
    ids: Arc<AtomicU64>,
    cipher: Arc<crypto::Cipher>,
}

struct Source {
    id: String,
    connector: String,
    /// Options as stored: values of `secret_keys` are AES-GCM ciphertext.
    options: HashMap<String, String>,
    secret_keys: Vec<String>,
    tables: Vec<TableSchema>,
    syncs: HashMap<String, SyncState>,
}

/// Decrypt a source's secret option values back to plaintext for use.
fn plaintext_options(
    src: &Source,
    cipher: &crypto::Cipher,
) -> Result<HashMap<String, String>, String> {
    let mut out = src.options.clone();
    for k in &src.secret_keys {
        if let Some(v) = out.get(k).cloned() {
            out.insert(k.clone(), cipher.decrypt(&v)?);
        }
    }
    Ok(out)
}

#[derive(Clone)]
struct SyncState {
    enabled: bool,
    interval_secs: u64,
    last_run_ms: Option<i64>,
    row_count: Option<i64>,
    status: String,
    last_run_instant: Option<Instant>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 60,
            last_run_ms: None,
            row_count: None,
            status: "idle".into(),
            last_run_instant: None,
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ─────────────────────────── JSON serialization ───────────────────────────

fn table_json(t: &TableSchema) -> serde_json::Value {
    json!({
        "name": t.name,
        "columns": t.columns.iter().map(|c| json!({
            "name": c.name,
            "type": connector_sdk::pg_type(c.data_type),
        })).collect::<Vec<_>>(),
    })
}

fn sync_json(table: &str, s: &SyncState) -> serde_json::Value {
    json!({
        "table": table,
        "enabled": s.enabled,
        "intervalSecs": s.interval_secs,
        "lastRunMs": s.last_run_ms,
        "rowCount": s.row_count,
        "status": s.status,
    })
}

fn source_json(src: &Source) -> serde_json::Value {
    let mut syncs: Vec<_> = src.syncs.iter().map(|(t, s)| sync_json(t, s)).collect();
    syncs.sort_by(|a, b| a["table"].as_str().cmp(&b["table"].as_str()));
    json!({
        "id": src.id,
        "connector": src.connector,
        "tables": src.tables.iter().map(table_json).collect::<Vec<_>>(),
        "syncs": syncs,
    })
}

fn err(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(json!({ "error": msg.into() }))).into_response()
}

// ─────────────────────────────── engine ───────────────────────────────

/// (key, required, secret) for a connector — from the REST catalog, or the
/// GraphQL registry for GraphQL connectors.
fn connector_options(name: &str) -> Vec<(&'static str, bool, bool)> {
    match name {
        "monday" => vec![("token", true, true), ("base_url", false, false)],
        _ => catalog::options_for(name)
            .into_iter()
            .map(|o| (o.key, o.required, o.secret))
            .collect(),
    }
}

/// Build a connector — REST (via the catalog) or GraphQL — behind the trait.
fn build_connector(
    connector: &str,
    options: &HashMap<String, String>,
) -> Result<Box<dyn Connector>, String> {
    if GRAPHQL_CONNECTORS.contains(&connector) {
        match connector {
            "monday" => {
                let token = options
                    .get("token")
                    .ok_or_else(|| "connector 'monday' requires the 'token' option".to_string())?;
                let base_url = options
                    .get("base_url")
                    .cloned()
                    .unwrap_or_else(|| "https://api.monday.com/v2".to_string());
                Ok(Box::new(GraphQlConnector::new(monday_spec(
                    &MondayConfig {
                        base_url,
                        token: token.clone(),
                    },
                ))))
            }
            other => Err(format!("unknown connector: {other}")),
        }
    } else {
        let spec = spec_for(connector, options).map_err(|e| e.to_string())?;
        Ok(Box::new(RestConnector::new(spec)))
    }
}

/// Full-refresh a table into its shadow table. Returns the row count.
async fn run_sync(
    pool: &Pool,
    id: &str,
    connector: &str,
    options: &HashMap<String, String>,
    schema: &TableSchema,
) -> Result<i64, String> {
    let conn = build_connector(connector, options)?;
    let query = Query {
        limit: Some(SYNC_LIMIT),
        ..Default::default()
    };
    let rows = conn
        .fetch(&schema.name, &query)
        .await
        .map_err(|e| e.to_string())?;

    let client = pool.get().await.map_err(|e| e.to_string())?;
    let tbl = sql::quote_ident(&sql::shadow_table(id, &schema.name));
    client
        .batch_execute("CREATE SCHEMA IF NOT EXISTS shadow")
        .await
        .map_err(|e| e.to_string())?;
    client
        .batch_execute(&format!("DROP TABLE IF EXISTS shadow.{tbl}"))
        .await
        .map_err(|e| e.to_string())?;
    client
        .batch_execute(&sql::create_shadow_ddl(id, &schema.name, schema))
        .await
        .map_err(|e| e.to_string())?;
    if let Some(insert) = sql::insert_rows_sql(id, &schema.name, schema, &rows) {
        client
            .batch_execute(&insert)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(rows.len() as i64)
}

/// Mutate one table's sync state under the write lock.
async fn with_sync<F: FnOnce(&mut SyncState)>(st: &AppState, id: &str, table: &str, f: F) {
    let mut g = st.sources.write().await;
    if let Some(src) = g.get_mut(id) {
        f(src.syncs.entry(table.to_string()).or_default());
    }
}

/// Run a sync and record the outcome on the sync state.
async fn run_and_record(
    st: &AppState,
    id: &str,
    connector: &str,
    options: &HashMap<String, String>,
    schema: &TableSchema,
) -> Result<i64, String> {
    with_sync(st, id, &schema.name, |s| s.status = "syncing".into()).await;
    match run_sync(&st.pool, id, connector, options, schema).await {
        Ok(n) => {
            let ms = now_ms();
            with_sync(st, id, &schema.name, |s| {
                s.row_count = Some(n);
                s.last_run_ms = Some(ms);
                s.last_run_instant = Some(Instant::now());
                s.status = "ok".into();
            })
            .await;
            Ok(n)
        }
        Err(e) => {
            let msg = format!("error: {e}");
            with_sync(st, id, &schema.name, |s| {
                s.last_run_instant = Some(Instant::now());
                s.status = msg.clone();
            })
            .await;
            Err(e)
        }
    }
}

/// Look up a source's connector/options and one table's schema.
async fn source_table(
    st: &AppState,
    id: &str,
    table: &str,
) -> Result<(String, HashMap<String, String>, TableSchema), Response> {
    let g = st.sources.read().await;
    let src = g
        .get(id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "unknown source"))?;
    let schema = src
        .tables
        .iter()
        .find(|t| t.name == table)
        .cloned()
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, format!("unknown table: {table}")))?;
    let opts = plaintext_options(src, &st.cipher)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok((src.connector.clone(), opts, schema))
}

// ─────────────────────────────── routes ───────────────────────────────

async fn get_connectors() -> Response {
    let connectors: Vec<_> = catalog::list()
        .iter()
        .copied()
        .chain(GRAPHQL_CONNECTORS.iter().copied())
        .map(|name| {
            let options: Vec<_> = connector_options(name)
                .into_iter()
                .map(|(key, required, secret)| json!({ "key": key, "required": required, "secret": secret }))
                .collect();
            json!({ "name": name, "options": options })
        })
        .collect();
    Json(json!({ "connectors": connectors })).into_response()
}

#[derive(Deserialize)]
struct MountReq {
    connector: String,
    #[serde(default)]
    options: HashMap<String, String>,
}

async fn post_source(State(st): State<AppState>, Json(req): Json<MountReq>) -> Response {
    let conn = match build_connector(&req.connector, &req.options) {
        Ok(c) => c,
        Err(e) => return err(StatusCode::BAD_REQUEST, e),
    };
    let tables = match conn.discover().await {
        Ok(t) => t,
        Err(e) => return err(StatusCode::BAD_REQUEST, e.to_string()),
    };

    // Validate the credentials actually have access: fetch one row from the
    // first table. Auth/permission problems surface here instead of at sync time.
    if let Some(first) = tables.first() {
        let probe = Query {
            limit: Some(1),
            ..Default::default()
        };
        if let Err(e) = conn.fetch(&first.name, &probe).await {
            return err(
                StatusCode::BAD_REQUEST,
                format!("credential check failed: {e}"),
            );
        }
    }

    // Encrypt secret option values before storing them.
    let secret_keys: Vec<String> = connector_options(&req.connector)
        .into_iter()
        .filter(|(_, _, secret)| *secret)
        .map(|(key, _, _)| key.to_string())
        .filter(|k| req.options.contains_key(k))
        .collect();
    let mut stored = req.options.clone();
    for k in &secret_keys {
        if let Some(v) = stored.get(k).cloned() {
            stored.insert(k.clone(), st.cipher.encrypt(&v));
        }
    }

    let id = format!("s{}", st.ids.fetch_add(1, Ordering::SeqCst) + 1);
    let src = Source {
        id: id.clone(),
        connector: req.connector,
        options: stored,
        secret_keys,
        tables,
        syncs: HashMap::new(),
    };
    let body = source_json(&src);
    st.sources.write().await.insert(id, src);
    Json(body).into_response()
}

async fn get_sources(State(st): State<AppState>) -> Response {
    let g = st.sources.read().await;
    let mut arr: Vec<_> = g.values().collect();
    arr.sort_by(|a, b| a.id.cmp(&b.id));
    let sources: Vec<_> = arr.into_iter().map(source_json).collect();
    Json(json!({ "sources": sources })).into_response()
}

#[derive(Deserialize)]
struct SyncReq {
    table: String,
    enabled: bool,
    #[serde(rename = "intervalSecs")]
    interval_secs: u64,
}

async fn post_sync(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SyncReq>,
) -> Response {
    let (connector, options, schema) = match source_table(&st, &id, &req.table).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    with_sync(&st, &id, &req.table, |s| {
        s.enabled = req.enabled;
        s.interval_secs = req.interval_secs.max(5);
    })
    .await;
    if req.enabled {
        let _ = run_and_record(&st, &id, &connector, &options, &schema).await;
    }
    let g = st.sources.read().await;
    let state = g
        .get(&id)
        .and_then(|s| s.syncs.get(&req.table))
        .cloned()
        .unwrap_or_default();
    Json(json!({ "sync": sync_json(&req.table, &state) })).into_response()
}

async fn post_refresh(
    State(st): State<AppState>,
    Path((id, table)): Path<(String, String)>,
) -> Response {
    let (connector, options, schema) = match source_table(&st, &id, &table).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match run_and_record(&st, &id, &connector, &options, &schema).await {
        Ok(n) => Json(json!({ "rowCount": n })).into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct DataQ {
    limit: Option<usize>,
}

async fn get_data(
    State(st): State<AppState>,
    Path((id, table)): Path<(String, String)>,
    AxumQuery(q): AxumQuery<DataQ>,
) -> Response {
    let (_connector, _options, schema) = match source_table(&st, &id, &table).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let limit = q.limit.unwrap_or(50).min(1000);
    let client = match st.pool.get().await {
        Ok(c) => c,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let query = sql::select_text_sql(&id, &table, &schema, limit);
    let rows = match client.query(query.as_str(), &[]).await {
        Ok(r) => r,
        Err(_) => return err(StatusCode::BAD_REQUEST, "not synced yet — run a sync first"),
    };
    let ncols = schema.columns.len();
    let data: Vec<Vec<Option<String>>> = rows
        .iter()
        .map(|r| (0..ncols).map(|i| r.get::<_, Option<String>>(i)).collect())
        .collect();
    Json(json!({
        "columns": schema.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        "rows": data,
    }))
    .into_response()
}

// ────────────────────────────── scheduler ──────────────────────────────

fn spawn_scheduler(st: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3));
        loop {
            tick.tick().await;
            let due: Vec<(String, String, HashMap<String, String>, TableSchema)> = {
                let g = st.sources.read().await;
                let mut v = Vec::new();
                for src in g.values() {
                    for (tname, s) in &src.syncs {
                        if !s.enabled {
                            continue;
                        }
                        let is_due = match s.last_run_instant {
                            None => true,
                            Some(i) => i.elapsed() >= Duration::from_secs(s.interval_secs),
                        };
                        if is_due {
                            if let Some(schema) = src.tables.iter().find(|t| &t.name == tname) {
                                if let Ok(opts) = plaintext_options(src, &st.cipher) {
                                    v.push((
                                        src.id.clone(),
                                        src.connector.clone(),
                                        opts,
                                        schema.clone(),
                                    ));
                                }
                            }
                        }
                    }
                }
                v
            };
            for (id, connector, options, schema) in due {
                // Stamp the instant up front so a slow sync isn't re-triggered.
                with_sync(&st, &id, &schema.name, |s| {
                    s.last_run_instant = Some(Instant::now())
                })
                .await;
                let _ = run_and_record(&st, &id, &connector, &options, &schema).await;
            }
        }
    });
}

// ──────────────────────────────── main ────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost:5432/budbuk".into());
    let mut cfg = PgConfig::new();
    cfg.url = Some(db_url.clone());
    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
    // Fail fast if the DB is unreachable, and ensure the shadow schema exists.
    pool.get()
        .await?
        .batch_execute("CREATE SCHEMA IF NOT EXISTS shadow")
        .await?;

    let state = AppState {
        pool,
        sources: Arc::new(RwLock::new(HashMap::new())),
        ids: Arc::new(AtomicU64::new(0)),
        cipher: Arc::new(crypto::Cipher::from_env()),
    };
    spawn_scheduler(state.clone());

    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "../web/dist".into());
    let index = format!("{static_dir}/index.html");

    let app = Router::new()
        .route("/api/connectors", get(get_connectors))
        .route("/api/sources", get(get_sources).post(post_source))
        .route("/api/sources/:id/syncs", post(post_sync))
        .route("/api/sources/:id/tables/:table/refresh", post(post_refresh))
        .route("/api/sources/:id/tables/:table/data", get(get_data))
        .fallback_service(ServeDir::new(&static_dir).not_found_service(ServeFile::new(index)))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("BudBuk control panel on http://localhost:{port}  (db: {db_url})");
    axum::serve(listener, app).await?;
    Ok(())
}
