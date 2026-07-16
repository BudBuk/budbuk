import { useEffect, useMemo, useState } from 'react'
import { createSource, type Connector } from '../api'

interface Props {
  connector: Connector | null
  onMounted: () => void
}

// Human-friendly labels for known option keys. Anything not listed falls back
// to a sentence-cased version of the raw key.
const LABELS: Record<string, string> = {
  api_key: 'API key',
  api_token: 'API token',
  base_url: 'Base URL',
  app_password: 'App password',
  account_sid: 'Account SID',
  auth_token: 'Auth token',
  access_token: 'Access token',
  consumer_key: 'Consumer key',
  consumer_secret: 'Consumer secret',
  tenant_id: 'Tenant ID',
  app_key: 'App key',
  owner: 'Owner',
  repo: 'Repo',
  email: 'Email',
  username: 'Username',
  password: 'Password',
  token: 'Token',
  spec: 'Spec',
}

function humanizeLabel(key: string): string {
  const override = LABELS[key]
  if (override) return override
  const words = key.split(/[_\s]+/).filter(Boolean)
  if (words.length === 0) return key
  return words
    .map((w, i) => (i === 0 ? w.charAt(0).toUpperCase() + w.slice(1) : w))
    .join(' ')
}

function placeholderFor(key: string): string {
  if (key === 'base_url' || key.endsWith('_url') || key === 'url') {
    return 'https://…'
  }
  if (key === 'email') return 'you@example.com'
  return `Enter ${humanizeLabel(key).toLowerCase()}`
}

export default function MountSource({ connector, onMounted }: Props) {
  const [values, setValues] = useState<Record<string, string>>({})
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  // Reset the form whenever a different connector is selected.
  const connectorName = connector?.name ?? null
  useEffect(() => {
    setValues({})
    setError(null)
  }, [connectorName])

  const options = connector?.options ?? []

  const missingRequired = useMemo(
    () =>
      options.some(
        (o) => o.required && (values[o.key] ?? '').trim() === '',
      ),
    [options, values],
  )

  function setValue(key: string, value: string) {
    setValues((prev) => ({ ...prev, [key]: value }))
  }

  async function handleMount() {
    if (!connector || missingRequired) return
    setBusy(true)
    setError(null)

    // Build the options payload: required fields (validated non-empty) plus
    // any optional field the user actually filled in.
    const payload: Record<string, string> = {}
    for (const o of options) {
      const v = (values[o.key] ?? '').trim()
      if (v !== '') payload[o.key] = v
    }

    try {
      await createSource(connector.name, payload)
      setValues({})
      setError(null)
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
      {!connector ? (
        <p className="muted">Select a connector above to mount a source.</p>
      ) : (
        <>
          <p className="mount-connector">
            Connector: <strong>{connector.name}</strong>
          </p>

          {options.length === 0 ? (
            <p className="muted small">
              This connector takes no options — just mount it.
            </p>
          ) : (
            <div className="field-list">
              {options.map((o) => {
                const label = humanizeLabel(o.key)
                return (
                  <label key={o.key} className="field">
                    <span className="field-label">
                      {label}
                      {o.required ? (
                        <span className="field-required" aria-hidden="true">
                          {' '}
                          *
                        </span>
                      ) : (
                        <span className="field-optional"> (optional)</span>
                      )}
                    </span>
                    <input
                      className="input field-input"
                      type={o.secret ? 'password' : 'text'}
                      value={values[o.key] ?? ''}
                      required={o.required}
                      autoComplete={o.secret ? 'new-password' : 'off'}
                      placeholder={placeholderFor(o.key)}
                      onChange={(e) => setValue(o.key, e.target.value)}
                    />
                  </label>
                )
              })}
            </div>
          )}

          <div className="form-actions">
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleMount}
              disabled={busy || missingRequired}
            >
              {busy ? 'Mounting…' : 'Mount'}
            </button>
          </div>

          {error && <p className="error mount-error">{error}</p>}
        </>
      )}
    </section>
  )
}
