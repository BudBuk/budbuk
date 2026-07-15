//! A framework-level cache with TTL and stale-while-revalidate (SWR).
//!
//! Goal: repeated queries are served instantly and we stop hammering the source
//! API (saving latency and rate limits). Two pieces:
//!   - `Cache`: the shared store (key -> rows + timestamp).
//!   - `CachedConnector<C>`: wraps ANY `Connector` and adds caching. Because it
//!     *also* implements `Connector`, callers can't tell it's there — the
//!     "decorator" pattern.
//!
//! New Rust concepts:
//!   - `Arc<Mutex<T>>`: `Arc` = shared ownership across threads/tasks; `Mutex`
//!     = one-at-a-time access so there are no data races. Together they make
//!     shared mutable state safe.
//!   - `Instant`: a monotonic clock reading; `.elapsed()` gives an entry's age.
//!   - `tokio::spawn`: run a task in the background (used to refresh stale data
//!     without making the caller wait).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::connector::Connector;
use crate::error::Result;
use crate::types::{Query, Row, TableSchema};

/// One cached result set plus when it was stored.
struct Entry {
    rows: Vec<Row>,
    stored_at: Instant,
}

/// A cheap-to-clone handle to a shared cache store. Cloning shares the SAME
/// underlying data (that's what `Arc` does), so several connectors can share
/// one cache while keeping entries separate via namespaced keys.
#[derive(Clone, Default)]
pub struct Cache {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    /// Keys currently being refreshed in the background — prevents two tasks
    /// from refetching the same key at once (the "thundering herd" problem).
    in_flight: Arc<Mutex<HashSet<String>>>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a clone of the cached rows and their age, if the key exists.
    fn lookup(&self, key: &str) -> Option<(Vec<Row>, Duration)> {
        let entries = self.entries.lock().unwrap();
        entries
            .get(key)
            .map(|e| (e.rows.clone(), e.stored_at.elapsed()))
    }

    /// Store (or replace) an entry, timestamped as of now.
    fn store(&self, key: String, rows: Vec<Row>) {
        let mut entries = self.entries.lock().unwrap();
        entries.insert(
            key,
            Entry {
                rows,
                stored_at: Instant::now(),
            },
        );
    }

    /// Try to claim a background refresh for `key`. `HashSet::insert` returns
    /// true only if the key was newly added — so this is true for the first
    /// caller and false while a refresh is already in flight.
    fn begin_refresh(&self, key: &str) -> bool {
        self.in_flight.lock().unwrap().insert(key.to_string())
    }

    fn end_refresh(&self, key: &str) {
        self.in_flight.lock().unwrap().remove(key);
    }
}

/// Wraps a connector and serves its `fetch` results from a `Cache`.
pub struct CachedConnector<C> {
    inner: Arc<C>,
    cache: Cache,
    /// Identifies THIS account, so two accounts sharing one cache never collide
    /// (e.g. `work.atlassian.net` vs `side.atlassian.net`).
    namespace: String,
    /// How long a result is considered fresh.
    ttl: Duration,
    /// After `ttl`, how long we'll still serve the stale copy (while refreshing
    /// in the background) before forcing a synchronous refetch.
    stale_window: Duration,
}

impl<C: Connector + 'static> CachedConnector<C> {
    pub fn new(
        inner: C,
        cache: Cache,
        namespace: impl Into<String>,
        ttl: Duration,
        stale_window: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(inner),
            cache,
            namespace: namespace.into(),
            ttl,
            stale_window,
        }
    }

    /// Build the namespaced cache key: account + table + query.
    fn key_for(&self, table: &str, query: &Query) -> String {
        format!("{}:{}:{}", self.namespace, table, query.cache_key())
    }

    /// Refresh one key in the background (unless a refresh is already running).
    /// We clone the `Arc<C>` and the `Cache` handle and move them into the task,
    /// so it can run independently of the request that triggered it.
    fn spawn_refresh(&self, key: String, table: String, query: Query) {
        if !self.cache.begin_refresh(&key) {
            return; // another task is already refreshing this key
        }
        let inner = Arc::clone(&self.inner);
        let cache = self.cache.clone();
        tokio::spawn(async move {
            if let Ok(rows) = inner.fetch(&table, &query).await {
                cache.store(key.clone(), rows);
            }
            cache.end_refresh(&key);
            eprintln!("[cache] background refresh complete: {key}");
        });
    }
}

#[async_trait]
impl<C: Connector + 'static> Connector for CachedConnector<C> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn discover(&self) -> Result<Vec<TableSchema>> {
        // Schemas are small and stable, so we just pass through for now.
        self.inner.discover().await
    }

    async fn fetch(&self, table: &str, query: &Query) -> Result<Vec<Row>> {
        let key = self.key_for(table, query);

        if let Some((rows, age)) = self.cache.lookup(&key) {
            if age < self.ttl {
                // FRESH: serve straight from cache — the fast path.
                eprintln!("[cache] HIT     {key} (age {age:?})");
                return Ok(rows);
            }
            if age < self.ttl + self.stale_window {
                // STALE: serve the old copy NOW, refresh in the background.
                // The caller never waits on the network.
                eprintln!("[cache] STALE   {key} (age {age:?}) — serving stale + refreshing");
                self.spawn_refresh(key, table.to_string(), query.clone());
                return Ok(rows);
            }
            eprintln!("[cache] EXPIRED {key} (age {age:?}) — refetching");
        } else {
            eprintln!("[cache] MISS    {key} — fetching");
        }

        // MISS or fully expired: fetch synchronously, store, return.
        let rows = self.inner.fetch(table, query).await?;
        self.cache.store(key, rows.clone());
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::{Cache, CachedConnector};
    use crate::connector::Connector;
    use crate::error::{ConnectorError, Result};
    use crate::types::{Query, Row, TableSchema, Value};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// A test connector that counts how many times `fetch` runs and returns the
    /// current count as its single cell — so tests can detect refetches.
    struct CountingConnector {
        name: String,
        calls: Arc<AtomicUsize>,
        schemas: Vec<TableSchema>,
        fail: bool,
    }

    impl CountingConnector {
        fn new(name: &str, calls: Arc<AtomicUsize>) -> Self {
            Self {
                name: name.to_string(),
                calls,
                schemas: vec![TableSchema::new("t", vec![])],
                fail: false,
            }
        }
    }

    #[async_trait]
    impl Connector for CountingConnector {
        fn name(&self) -> &str {
            &self.name
        }
        async fn discover(&self) -> Result<Vec<TableSchema>> {
            Ok(self.schemas.clone())
        }
        async fn fetch(&self, _table: &str, _query: &Query) -> Result<Vec<Row>> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if self.fail {
                return Err(ConnectorError::Other("boom".into()));
            }
            Ok(vec![Row(vec![Value::Integer(n as i64)])])
        }
    }

    /// Read the counter cell out of a fetched row set.
    fn cell(rows: &[Row]) -> i64 {
        rows[0].0[0].to_display_string().parse().unwrap()
    }

    /// Poll a condition for up to ~1s (spins the runtime so background tasks run),
    /// then assert it held — the assert line always runs, so it's fully covered.
    async fn wait_for(cond: impl Fn() -> bool) {
        for _ in 0..200 {
            if cond() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(cond(), "condition not met in time");
    }

    fn counting(name: &str, calls: Arc<AtomicUsize>) -> CountingConnector {
        CountingConnector::new(name, calls)
    }

    #[tokio::test]
    async fn miss_then_hit_only_fetches_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = CachedConnector::new(
            counting("jira", calls.clone()),
            Cache::new(),
            "ns",
            Duration::from_secs(60),
            Duration::from_secs(60),
        );
        let q = Query::default();
        assert_eq!(cell(&c.fetch("t", &q).await.unwrap()), 1); // MISS
        assert_eq!(cell(&c.fetch("t", &q).await.unwrap()), 1); // HIT (no refetch)
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn name_and_discover_pass_through() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = CachedConnector::new(
            counting("jira", calls),
            Cache::default(),
            "ns",
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        assert_eq!(c.name(), "jira");
        assert_eq!(c.discover().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn namespacing_keeps_accounts_separate() {
        let cache = Cache::new();
        let (ca, cb) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let a = CachedConnector::new(
            counting("jira", ca.clone()),
            cache.clone(),
            "acctA",
            Duration::from_secs(60),
            Duration::from_secs(60),
        );
        let b = CachedConnector::new(
            counting("jira", cb.clone()),
            cache.clone(),
            "acctB",
            Duration::from_secs(60),
            Duration::from_secs(60),
        );
        let q = Query::default();
        a.fetch("t", &q).await.unwrap();
        b.fetch("t", &q).await.unwrap();
        // No cross-account cache hit: each account fetched from its own source.
        assert_eq!(ca.load(Ordering::SeqCst), 1);
        assert_eq!(cb.load(Ordering::SeqCst), 1);
        // And account A now HITs its own entry.
        a.fetch("t", &q).await.unwrap();
        assert_eq!(ca.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn expired_entry_is_refetched_synchronously() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = CachedConnector::new(
            counting("jira", calls.clone()),
            Cache::new(),
            "ns",
            Duration::from_millis(20),
            Duration::from_millis(20),
        );
        let q = Query::default();
        assert_eq!(cell(&c.fetch("t", &q).await.unwrap()), 1);
        tokio::time::sleep(Duration::from_millis(70)).await; // past ttl + stale_window
        assert_eq!(cell(&c.fetch("t", &q).await.unwrap()), 2); // fresh refetch
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn stale_serves_old_and_refreshes_in_background() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = CachedConnector::new(
            counting("jira", calls.clone()),
            Cache::new(),
            "ns",
            Duration::from_millis(40),
            Duration::from_secs(5),
        );
        let q = Query::default();
        assert_eq!(cell(&c.fetch("t", &q).await.unwrap()), 1); // MISS
        tokio::time::sleep(Duration::from_millis(70)).await; // into the stale window
        assert_eq!(cell(&c.fetch("t", &q).await.unwrap()), 1); // STALE: old value now
        wait_for(|| calls.load(Ordering::SeqCst) == 2).await; // background refresh ran
    }

    #[tokio::test]
    async fn stale_skips_refresh_when_one_is_already_in_flight() {
        let cache = Cache::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = CachedConnector::new(
            counting("jira", calls.clone()),
            cache.clone(),
            "ns",
            Duration::from_millis(30),
            Duration::from_secs(5),
        );
        let q = Query::default();
        c.fetch("t", &q).await.unwrap(); // MISS, calls = 1

        // Manually occupy the in-flight slot for this exact key so the stale
        // path finds a refresh "already running" and skips spawning one.
        let key = format!("ns:t:{}", q.cache_key());
        assert!(cache.begin_refresh(&key));

        tokio::time::sleep(Duration::from_millis(50)).await; // now stale
        c.fetch("t", &q).await.unwrap(); // STALE, but spawn is skipped
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1); // no background refresh happened

        cache.end_refresh(&key); // releasing lets a future refresh proceed
        assert!(cache.begin_refresh(&key)); // slot is free again
    }

    #[tokio::test]
    async fn fetch_error_from_source_propagates() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut inner = counting("jira", calls);
        inner.fail = true;
        let c = CachedConnector::new(
            inner,
            Cache::new(),
            "ns",
            Duration::from_secs(60),
            Duration::from_secs(60),
        );
        let err = c.fetch("t", &Query::default()).await.unwrap_err();
        assert!(matches!(err, ConnectorError::Other(_)));
    }
}
