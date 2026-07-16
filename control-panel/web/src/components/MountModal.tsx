import { useMemo, useState } from 'react'
import { createSource, type Connector } from '../api'
import { categoryColor, displayName, metaFor, websiteFor } from '../connectorMeta'
import BrandLogo from './BrandLogo'
import Modal from './Modal'
import { ExternalIcon, LockIcon } from './icons'

interface Props {
  connector: Connector
  onClose: () => void
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

export default function MountModal({ connector, onClose, onMounted }: Props) {
  const [values, setValues] = useState<Record<string, string>>({})
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const options = connector.options
  const meta = metaFor(connector.name)
  const color = categoryColor(meta.category)
  const site = websiteFor(connector.name)
  const label = displayName(connector.name)

  const missingRequired = useMemo(
    () => options.some((o) => o.required && (values[o.key] ?? '').trim() === ''),
    [options, values],
  )

  function setValue(key: string, value: string) {
    setValues((prev) => ({ ...prev, [key]: value }))
  }

  async function handleMount() {
    if (missingRequired || busy) return
    setBusy(true)
    setError(null)

    const payload: Record<string, string> = {}
    for (const o of options) {
      const v = (values[o.key] ?? '').trim()
      if (v !== '') payload[o.key] = v
    }

    try {
      await createSource(connector.name, payload)
      onMounted()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to mount source')
      setBusy(false)
    }
  }

  return (
    <Modal
      onClose={onClose}
      width={520}
      title={
        <div className="modal-title-row">
          <BrandLogo name={connector.name} size={40} />
          <div>
            <div className="modal-title">
              {site ? (
                <a
                  className="modal-title-link"
                  href={site}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  {label}
                  <ExternalIcon size={14} />
                </a>
              ) : (
                label
              )}
            </div>
            <span className="modal-cat" style={{ color }}>
              {meta.category}
            </span>
          </div>
        </div>
      }
      subtitle={meta.description}
    >
      {options.length === 0 ? (
        <p className="muted">This connector takes no options — just mount it.</p>
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
                  {o.secret && (
                    <span className="field-secret" title="Stored as a secret">
                      <LockIcon size={12} /> secret
                    </span>
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

      {error && <div className="banner banner-error">{error}</div>}

      <div className="modal-actions">
        <button type="button" className="btn btn-secondary" onClick={onClose} disabled={busy}>
          Cancel
        </button>
        <button
          type="button"
          className="btn btn-primary"
          onClick={handleMount}
          disabled={busy || missingRequired}
        >
          {busy && <span className="spinner" aria-hidden="true" />}
          {busy ? 'Mounting…' : 'Mount source'}
        </button>
      </div>
    </Modal>
  )
}
