import { useMemo } from 'react'
import { type Source } from '../api'
import { displayName } from '../connectorMeta'
import BrandLogo from './BrandLogo'
import StatusBadge, { formatTime, statusKind, type StatusKind } from './StatusBadge'
import { BoltIcon, ChartIcon, RowsIcon, SourcesIcon, TableIcon } from './icons'

interface Props {
  sources: Source[]
  loading: boolean
  error: string | null
}

const STATUS_META: { kind: StatusKind; label: string; color: string }[] = [
  { kind: 'ok', label: 'OK', color: 'var(--ok)' },
  { kind: 'syncing', label: 'Syncing', color: 'var(--blue)' },
  { kind: 'error', label: 'Error', color: 'var(--err)' },
  { kind: 'idle', label: 'Idle', color: 'var(--muted)' },
]

export default function Analytics({ sources, loading, error }: Props) {
  const model = useMemo(() => {
    let tables = 0
    let enabled = 0
    let totalRows = 0
    const statusCounts: Record<StatusKind, number> = { ok: 0, syncing: 0, error: 0, idle: 0 }

    const bySource = sources.map((s) => {
      let rows = 0
      for (const sync of s.syncs) rows += sync.rowCount ?? 0
      return { id: s.id, connector: s.connector, rows }
    })

    const recent: {
      key: string
      connector: string
      table: string
      rows: number
      status: string
      lastRunMs: number
    }[] = []

    for (const s of sources) {
      tables += s.tables.length
      for (const sync of s.syncs) {
        if (sync.enabled) enabled += 1
        totalRows += sync.rowCount ?? 0
        statusCounts[statusKind(sync.status)] += 1
        if (sync.lastRunMs != null) {
          recent.push({
            key: `${s.id}:${sync.table}`,
            connector: s.connector,
            table: sync.table,
            rows: sync.rowCount ?? 0,
            status: sync.status,
            lastRunMs: sync.lastRunMs,
          })
        }
      }
    }

    bySource.sort((a, b) => b.rows - a.rows)
    recent.sort((a, b) => b.lastRunMs - a.lastRunMs)

    const totalSyncs = statusCounts.ok + statusCounts.syncing + statusCounts.error + statusCounts.idle
    const maxRows = bySource.reduce((m, s) => Math.max(m, s.rows), 0)

    return {
      stats: { sources: sources.length, tables, enabled, totalRows },
      bySource,
      recent: recent.slice(0, 10),
      statusCounts,
      totalSyncs,
      maxRows,
    }
  }, [sources])

  const hasData = sources.length > 0

  return (
    <div className="view">
      <div className="view-head">
        <div>
          <h1 className="view-title">Analytics</h1>
          <p className="view-sub">A live view of sync activity across all sources.</p>
        </div>
      </div>

      {error && <div className="banner banner-error">{error}</div>}
      {loading && !hasData && <div className="placeholder">Loading analytics…</div>}

      {!loading && !hasData && !error && (
        <div className="empty">
          <div className="empty-icon">
            <ChartIcon size={26} />
          </div>
          <div className="empty-title">Nothing to chart yet</div>
          <p className="empty-sub">Mount a source and run a sync to see analytics here.</p>
        </div>
      )}

      {hasData && (
        <>
          <div className="stat-row">
            <StatCard icon={<SourcesIcon size={18} />} label="Sources" value={model.stats.sources} tone="blue" />
            <StatCard icon={<TableIcon size={18} />} label="Tables" value={model.stats.tables} tone="slate" />
            <StatCard icon={<BoltIcon size={18} />} label="Enabled syncs" value={model.stats.enabled} tone="green" />
            <StatCard
              icon={<RowsIcon size={18} />}
              label="Total rows synced"
              value={model.stats.totalRows.toLocaleString()}
              tone="orange"
            />
          </div>

          <div className="analytics-grid">
            <section className="card chart-card">
              <h2 className="card-title">Rows by source</h2>
              {model.maxRows === 0 ? (
                <p className="muted">No rows synced yet.</p>
              ) : (
                <div className="bars">
                  {model.bySource.map((s) => {
                    const pct = model.maxRows === 0 ? 0 : Math.round((s.rows / model.maxRows) * 100)
                    return (
                      <div key={s.id} className="bar-row">
                        <div className="bar-label">
                          <BrandLogo name={s.connector} size={22} />
                          <span className="bar-name">{displayName(s.connector)}</span>
                        </div>
                        <div className="bar-track">
                          <div
                            className="bar-fill"
                            style={{ width: `${Math.max(pct, s.rows > 0 ? 4 : 0)}%` }}
                          />
                        </div>
                        <span className="bar-value">{s.rows.toLocaleString()}</span>
                      </div>
                    )
                  })}
                </div>
              )}
            </section>

            <section className="card chart-card">
              <h2 className="card-title">Sync status</h2>
              {model.totalSyncs === 0 ? (
                <p className="muted">No syncs configured yet.</p>
              ) : (
                <>
                  <div className="status-track" role="img" aria-label="Sync status breakdown">
                    {STATUS_META.map((m) => {
                      const count = model.statusCounts[m.kind]
                      if (count === 0) return null
                      return (
                        <div
                          key={m.kind}
                          className="status-seg"
                          style={{
                            width: `${(count / model.totalSyncs) * 100}%`,
                            background: m.color,
                          }}
                          title={`${m.label}: ${count}`}
                        />
                      )
                    })}
                  </div>
                  <div className="status-legend">
                    {STATUS_META.map((m) => (
                      <div key={m.kind} className="legend-item">
                        <span className="legend-dot" style={{ background: m.color }} />
                        <span className="legend-label">{m.label}</span>
                        <span className="legend-count">{model.statusCounts[m.kind]}</span>
                      </div>
                    ))}
                  </div>
                </>
              )}
            </section>
          </div>

          <section className="card">
            <h2 className="card-title">Recent syncs</h2>
            {model.recent.length === 0 ? (
              <p className="muted">No syncs have run yet.</p>
            ) : (
              <div className="table-scroll">
                <table className="sync-table">
                  <thead>
                    <tr>
                      <th>Source</th>
                      <th>Table</th>
                      <th className="num">Rows</th>
                      <th>Status</th>
                      <th>Last run</th>
                    </tr>
                  </thead>
                  <tbody>
                    {model.recent.map((r) => (
                      <tr key={r.key}>
                        <td>
                          <div className="cell-source">
                            <BrandLogo name={r.connector} size={22} />
                            <span>{displayName(r.connector)}</span>
                          </div>
                        </td>
                        <td>
                          <span className="cell-table-name">{r.table}</span>
                        </td>
                        <td className="num">{r.rows.toLocaleString()}</td>
                        <td>
                          <StatusBadge status={r.status} />
                        </td>
                        <td className="cell-muted">{formatTime(r.lastRunMs)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>
        </>
      )}
    </div>
  )
}

function StatCard({
  icon,
  label,
  value,
  tone,
}: {
  icon: React.ReactNode
  label: string
  value: number | string
  tone: 'blue' | 'green' | 'orange' | 'slate'
}) {
  return (
    <div className="stat-card">
      <span className={`stat-icon stat-icon-${tone}`}>{icon}</span>
      <div className="stat-text">
        <div className="stat-value">{value}</div>
        <div className="stat-label">{label}</div>
      </div>
    </div>
  )
}
