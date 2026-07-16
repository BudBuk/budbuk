import { useState } from 'react'
import { createSource } from '../api'

interface OptionRow {
  key: string
  value: string
}

interface Props {
  selectedConnector: string | null
  onMounted: () => void
}

export default function MountSource({ selectedConnector, onMounted }: Props) {
  const [rows, setRows] = useState<OptionRow[]>([{ key: '', value: '' }])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  function updateRow(index: number, patch: Partial<OptionRow>) {
    setRows((prev) =>
      prev.map((r, i) => (i === index ? { ...r, ...patch } : r)),
    )
  }

  function addRow() {
    setRows((prev) => [...prev, { key: '', value: '' }])
  }

  function removeRow(index: number) {
    setRows((prev) => prev.filter((_, i) => i !== index))
  }

  function resetForm() {
    setRows([{ key: '', value: '' }])
    setError(null)
  }

  async function handleMount() {
    if (!selectedConnector) return
    setBusy(true)
    setError(null)
    const options: Record<string, string> = {}
    for (const r of rows) {
      if (r.key.trim() !== '') options[r.key.trim()] = r.value
    }
    try {
      await createSource(selectedConnector, options)
      resetForm()
      onMounted()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to mount source')
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="panel">
      <h2>Mount a source</h2>
      {!selectedConnector ? (
        <p className="muted">Select a connector above to mount a source.</p>
      ) : (
        <>
          <p>
            Connector: <strong>{selectedConnector}</strong>
          </p>
          <div className="options-editor">
            {rows.map((row, i) => (
              <div key={i} className="option-row">
                <input
                  className="input"
                  placeholder="key"
                  value={row.key}
                  onChange={(e) => updateRow(i, { key: e.target.value })}
                />
                <input
                  className="input"
                  placeholder="value"
                  value={row.value}
                  onChange={(e) => updateRow(i, { value: e.target.value })}
                />
                <button
                  type="button"
                  className="btn btn-ghost"
                  onClick={() => removeRow(i)}
                  disabled={rows.length === 1}
                  aria-label="Remove option"
                >
                  ×
                </button>
              </div>
            ))}
          </div>
          <div className="form-actions">
            <button type="button" className="btn btn-ghost" onClick={addRow}>
              + Add option
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleMount}
              disabled={busy}
            >
              {busy ? 'Mounting…' : 'Mount'}
            </button>
          </div>
          {error && <p className="error">{error}</p>}
        </>
      )}
    </section>
  )
}
