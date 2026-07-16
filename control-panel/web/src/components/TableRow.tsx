import { useState, type ChangeEvent } from 'react'
import {
  getTableData,
  refreshTable,
  upsertSync,
  type DataPreview,
  type SyncState,
  type TableSchema,
} from '../api'

interface Props {
  sourceId: string
  table: TableSchema
  sync: SyncState | undefined
  onChanged: () => void
}

function formatLastRun(lastRunMs: number | null): string {
  if (lastRunMs == null) return 'never'
  return new Date(lastRunMs).toLocaleTimeString()
}

export default function TableRow({ sourceId, table, sync, onChanged }: Props) {
  const enabled = sync?.enabled ?? false
  const intervalSecs = sync?.intervalSecs ?? 60

  const [interval, setInterval] = useState<number>(intervalSecs)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [preview, setPreview] = useState<DataPreview | null>(null)
  const [showPreview, setShowPreview] = useState(false)

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

  function handleToggle(e: ChangeEvent<HTMLInputElement>) {
    void saveSync(e.target.checked, interval)
  }

  function handleIntervalChange(e: ChangeEvent<HTMLInputElement>) {
    setInterval(Number(e.target.value))
  }

  function handleIntervalCommit() {
    void saveSync(enabled, interval)
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
    if (showPreview) {
      setShowPreview(false)
      return
    }
    setBusy(true)
    setError(null)
    try {
      const data = await getTableData(sourceId, table.name, 50)
      setPreview(data)
      setShowPreview(true)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load data')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="table-row">
      <div className="table-head">
        <div className="table-title">
          <span className="table-name">{table.name}</span>
          <span className="muted small">
            {table.columns.length} column{table.columns.length === 1 ? '' : 's'}
          </span>
        </div>

        <div className="table-controls">
          <label className="control">
            <input type="checkbox" checked={enabled} onChange={handleToggle} />
            <span>Enabled</span>
          </label>

          <label className="control">
            <span>Interval (s)</span>
            <input
              className="input input-num"
              type="number"
              min={1}
              value={interval}
              onChange={handleIntervalChange}
              onBlur={handleIntervalCommit}
            />
          </label>

          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={handleSyncNow}
            disabled={busy}
          >
            Sync now
          </button>

          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={handlePreview}
            disabled={busy}
          >
            {showPreview ? 'Hide' : 'Preview'}
          </button>
        </div>
      </div>

      <div className="table-status">
        {sync ? (
          <span>
            <span className={`status status-${sync.status}`}>{sync.status}</span>
            {' · '}
            {sync.rowCount ?? 0} rows
            {' · '}
            last run {formatLastRun(sync.lastRunMs)}
          </span>
        ) : (
          <span className="muted small">No sync configured</span>
        )}
        {error && <span className="error small"> · {error}</span>}
      </div>

      {showPreview && preview && (
        <div className="preview">
          <table className="data-table">
            <thead>
              <tr>
                {preview.columns.map((c) => (
                  <th key={c}>{c}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {preview.rows.map((row, ri) => (
                <tr key={ri}>
                  {row.map((cell, ci) => (
                    <td key={ci}>
                      {cell === null ? <span className="null">null</span> : cell}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
          {preview.rows.length === 0 && (
            <p className="muted small">No rows.</p>
          )}
        </div>
      )}
    </div>
  )
}
