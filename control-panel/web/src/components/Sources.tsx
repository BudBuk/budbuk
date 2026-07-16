import { type Source } from '../api'
import TableRow from './TableRow'

interface Props {
  sources: Source[]
  loading: boolean
  error: string | null
  onChanged: () => void
}

export default function Sources({ sources, loading, error, onChanged }: Props) {
  return (
    <section className="panel">
      <h2>Sources</h2>
      {loading && sources.length === 0 && (
        <p className="muted">Loading sources…</p>
      )}
      {error && <p className="error">{error}</p>}
      {!loading && !error && sources.length === 0 && (
        <p className="muted">No sources mounted yet.</p>
      )}

      <div className="sources-list">
        {sources.map((source) => (
          <div key={source.id} className="source-card">
            <div className="source-head">
              <span className="badge">{source.connector}</span>
              <span className="muted small">{source.id}</span>
            </div>
            <div className="tables">
              {source.tables.map((table) => {
                const sync = source.syncs.find((s) => s.table === table.name)
                return (
                  <TableRow
                    key={table.name}
                    sourceId={source.id}
                    table={table}
                    sync={sync}
                    onChanged={onChanged}
                  />
                )
              })}
              {source.tables.length === 0 && (
                <p className="muted small">No tables.</p>
              )}
            </div>
          </div>
        ))}
      </div>
    </section>
  )
}
