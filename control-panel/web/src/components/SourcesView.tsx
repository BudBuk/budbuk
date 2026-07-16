import { useMemo, useState } from 'react'
import {
  getTableData,
  refreshTable,
  upsertSync,
  type DataPreview,
  type Source,
  type SyncState,
  type TableSchema,
} from '../api'
import { displayName } from '../connectorMeta'
import BrandLogo from './BrandLogo'
import PreviewModal from './PreviewModal'
import StatusBadge, { formatTime } from './StatusBadge'
import { BoltIcon, LayersIcon, RefreshIcon, RowsIcon, SourcesIcon, TableIcon } from './icons'

interface Props {
  sources: Source[]
  loading: boolean
  error: string | null
  onChanged: () => void
  onGoToCatalog: () => void
}

interface PreviewTarget {
  title: string
  data: DataPreview
}

export default function SourcesView({ sources, loading, error, onChanged, onGoToCatalog }: Props) {
  const [preview, setPreview] = useState<PreviewTarget | null>(null)

  const stats = useMemo(() => {
    let tables = 0
    let enabled = 0
    let rows = 0
    for (const s of sources) {
      tables += s.tables.length
      for (const sync of s.syncs) {
        if (sync.enabled) enabled += 1
        rows += sync.rowCount ?? 0
      }
    }
    return { sources: sources.length, tables, enabled, rows }
  }, [sources])

  return (
    <div className="view">
      <div className="view-head">
        <div>
          <h1 className="view-title">Sources</h1>
          <p className="view-sub">Mounted connectors and their synced tables.</p>
        </div>
      </div>

      {error && <div className="banner banner-error">{error}</div>}

      {sources.length > 0 && (
        <div className="stat-row">
          <StatCard icon={<SourcesIcon size={18} />} label="Sources" value={stats.sources} tone="blue" />
          <StatCard icon={<TableIcon size={18} />} label="Tables" value={stats.tables} tone="slate" />
          <StatCard icon={<BoltIcon size={18} />} label="Enabled syncs" value={stats.enabled} tone="green" />
          <StatCard icon={<RowsIcon size={18} />} label="Rows synced" value={stats.rows.toLocaleString()} tone="orange" />
        </div>
      )}

      {loading && sources.length === 0 && <div className="placeholder">Loading sources…</div>}

      {!loading && !error && sources.length === 0 && (
        <div className="empty">
          <div className="empty-icon">
            <LayersIcon size={26} />
          </div>
          <div className="empty-title">No sources yet</div>
          <p className="empty-sub">Mount one from the Catalog to start syncing data.</p>
          <button type="button" className="btn btn-primary" onClick={onGoToCatalog}>
            Browse Catalog
          </button>
        </div>
      )}

      <div className="source-list">
        {sources.map((source) => (
          <section key={source.id} className="card source-card">
            <div className="source-head">
              <BrandLogo name={source.connector} size={38} />
              <div className="source-head-text">
                <div className="source-name">{displayName(source.connector)}</div>
                <div className="source-id">{source.id}</div>
              </div>
              <span className="source-meta">
                {source.tables.length} table{source.tables.length === 1 ? '' : 's'}
              </span>
            </div>

            {source.tables.length === 0 ? (
              <p className="muted source-empty">No tables.</p>
            ) : (
              <div className="table-scroll">
                <table className="sync-table">
                  <thead>
                    <tr>
                      <th>Table</th>
                      <th className="num">Cols</th>
                      <th>Sync</th>
                      <th className="num">Interval</th>
                      <th>Status</th>
                      <th>Last run</th>
                      <th className="num">Rows</th>
                      <th className="actions-col">Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {source.tables.map((table) => (
                      <SyncRow
                        key={table.name}
                        sourceId={source.id}
                        connector={source.connector}
                        table={table}
                        sync={source.syncs.find((s) => s.table === table.name)}
                        onChanged={onChanged}
                        onPreview={setPreview}
                      />
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>
        ))}
      </div>

      {preview && (
        <PreviewModal title={preview.title} preview={preview.data} onClose={() => setPreview(null)} />
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

interface RowProps {
  sourceId: string
  connector: string
  table: TableSchema
  sync: SyncState | undefined
  onChanged: () => void
  onPreview: (t: PreviewTarget) => void
}

function SyncRow({ sourceId, connector, table, sync, onChanged, onPreview }: RowProps) {
  const enabled = sync?.enabled ?? false
  const [interval, setIntervalValue] = useState<number>(sync?.intervalSecs ?? 60)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function saveSync(nextEnabled: boolean, nextInterval: number) {
    setBusy(true)
    setError(null)
    try {
      await upsertSync(sourceId, table.name, nextEnabled, nextInterval)
      onChanged()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to update sync')
    } finally {
      setBusy(false)
    }
  }

  async function handleSyncNow() {
    setBusy(true)
    setError(null)
    try {
      await refreshTable(sourceId, table.name)
      onChanged()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to sync')
    } finally {
      setBusy(false)
    }
  }

  async function handlePreview() {
    setBusy(true)
    setError(null)
    try {
      const data = await getTableData(sourceId, table.name, 50)
      onPreview({ title: `${displayName(connector)} · ${table.name}`, data })
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load data')
    } finally {
      setBusy(false)
    }
  }

  return (
    <tr>
      <td>
        <span className="cell-table-name">{table.name}</span>
        {error && <span className="cell-error">{error}</span>}
      </td>
      <td className="num">{table.columns.length}</td>
      <td>
        <label className="switch">
          <input
            type="checkbox"
            checked={enabled}
            disabled={busy}
            onChange={(e) => void saveSync(e.target.checked, interval)}
          />
          <span className="switch-track">
            <span className="switch-thumb" />
          </span>
        </label>
      </td>
      <td className="num">
        <input
          className="input input-num"
          type="number"
          min={1}
          value={interval}
          disabled={busy}
          onChange={(e) => setIntervalValue(Number(e.target.value))}
          onBlur={() => {
            if (interval !== (sync?.intervalSecs ?? 60)) void saveSync(enabled, interval)
          }}
        />
      </td>
      <td>
        <StatusBadge status={sync?.status} />
      </td>
      <td className="cell-muted">{formatTime(sync?.lastRunMs)}</td>
      <td className="num">{(sync?.rowCount ?? 0).toLocaleString()}</td>
      <td className="actions-col">
        <div className="row-actions">
          <button
            type="button"
            className="btn btn-ghost btn-icon btn-sm"
            onClick={handleSyncNow}
            disabled={busy}
            title="Sync now"
          >
            <RefreshIcon size={15} />
            Sync
          </button>
          <button
            type="button"
            className="btn btn-secondary btn-sm"
            onClick={handlePreview}
            disabled={busy}
          >
            Preview
          </button>
        </div>
      </td>
    </tr>
  )
}
