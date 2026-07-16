// Normalizes the free-form backend status string into one of four buckets and
// renders a colored badge. Anything starting with "error" is treated as an
// error and its full text shown.
export type StatusKind = 'idle' | 'syncing' | 'ok' | 'error'

export function statusKind(status: string | undefined | null): StatusKind {
  const s = (status ?? '').toLowerCase()
  if (s.startsWith('error') || s === 'failed') return 'error'
  if (s === 'syncing' || s === 'running') return 'syncing'
  if (s === 'ok' || s === 'success' || s === 'synced') return 'ok'
  return 'idle'
}

export function statusLabel(status: string | undefined | null): string {
  const kind = statusKind(status)
  if (kind === 'error') return status || 'error'
  if (kind === 'ok') return 'ok'
  if (kind === 'syncing') return 'syncing'
  return status && status.trim() !== '' ? status : 'idle'
}

export default function StatusBadge({ status }: { status: string | undefined | null }) {
  const kind = statusKind(status)
  return (
    <span className={`status-badge status-${kind}`}>
      <span className="status-dot" />
      {statusLabel(status)}
    </span>
  )
}

export function formatTime(lastRunMs: number | null | undefined): string {
  if (lastRunMs == null) return 'never'
  const d = new Date(lastRunMs)
  const now = Date.now()
  const diff = now - lastRunMs
  if (diff >= 0 && diff < 60_000) return 'just now'
  if (diff >= 0 && diff < 3_600_000) {
    const m = Math.floor(diff / 60_000)
    return `${m}m ago`
  }
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}
