// Typed client for the BudBuk backend API.

export interface ConnectorOption {
  key: string
  required: boolean
  secret: boolean
}

export interface Connector {
  name: string
  options: ConnectorOption[]
}

export interface Column {
  name: string
  type: string
}

export interface TableSchema {
  name: string
  columns: Column[]
}

export interface SyncState {
  table: string
  enabled: boolean
  intervalSecs: number
  lastRunMs: number | null
  rowCount: number | null
  status: string
}

export interface Source {
  id: string
  connector: string
  tables: TableSchema[]
  syncs: SyncState[]
}

export interface DataPreview {
  columns: string[]
  rows: (string | null)[][]
}

interface ApiError {
  error?: string
}

async function handle<T>(res: Response): Promise<T> {
  if (!res.ok) {
    let message = `Request failed (${res.status})`
    try {
      const body = (await res.json()) as ApiError
      if (body && body.error) message = body.error
    } catch {
      // ignore JSON parse errors, keep default message
    }
    throw new Error(message)
  }
  return (await res.json()) as T
}

export async function getConnectors(): Promise<Connector[]> {
  const res = await fetch('/api/connectors')
  const body = await handle<{ connectors: Connector[] }>(res)
  return body.connectors
}

export async function createSource(
  connector: string,
  options: Record<string, string>,
): Promise<Source> {
  const res = await fetch('/api/sources', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ connector, options }),
  })
  return handle<Source>(res)
}

export async function getSources(): Promise<Source[]> {
  const res = await fetch('/api/sources')
  const body = await handle<{ sources: Source[] }>(res)
  return body.sources
}

export async function upsertSync(
  sourceId: string,
  table: string,
  enabled: boolean,
  intervalSecs: number,
): Promise<SyncState> {
  const res = await fetch(`/api/sources/${encodeURIComponent(sourceId)}/syncs`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ table, enabled, intervalSecs }),
  })
  const body = await handle<{ sync: SyncState }>(res)
  return body.sync
}

export async function refreshTable(
  sourceId: string,
  table: string,
): Promise<number> {
  const res = await fetch(
    `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/refresh`,
    { method: 'POST' },
  )
  const body = await handle<{ rowCount: number }>(res)
  return body.rowCount
}

export async function getTableData(
  sourceId: string,
  table: string,
  limit = 50,
): Promise<DataPreview> {
  const res = await fetch(
    `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/data?limit=${limit}`,
  )
  return handle<DataPreview>(res)
}
