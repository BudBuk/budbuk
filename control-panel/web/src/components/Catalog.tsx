import { useMemo, useState } from 'react'
import { type Connector } from '../api'
import { CATEGORIES, categoryColor, displayName, metaFor, websiteFor } from '../connectorMeta'
import BrandLogo from './BrandLogo'
import { ExternalIcon, SearchIcon } from './icons'

interface Props {
  connectors: Connector[]
  loading: boolean
  error: string | null
  onOpen: (connector: Connector) => void
}

const ALL = '__all__'

export default function Catalog({ connectors, loading, error, onOpen }: Props) {
  const [query, setQuery] = useState('')
  const [activeCategory, setActiveCategory] = useState<string>(ALL)

  const decorated = useMemo(
    () =>
      connectors.map((c) => {
        const meta = metaFor(c.name)
        return { connector: c, ...meta }
      }),
    [connectors],
  )

  const categoryCounts = useMemo(() => {
    const counts = new Map<string, number>()
    for (const d of decorated) {
      counts.set(d.category, (counts.get(d.category) ?? 0) + 1)
    }
    return counts
  }, [decorated])

  const visibleCategories = useMemo(
    () => CATEGORIES.filter((c) => categoryCounts.has(c)),
    [categoryCounts],
  )

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    return decorated.filter((d) => {
      if (activeCategory !== ALL && d.category !== activeCategory) return false
      if (q === '') return true
      return (
        d.connector.name.toLowerCase().includes(q) ||
        d.description.toLowerCase().includes(q) ||
        d.category.toLowerCase().includes(q)
      )
    })
  }, [decorated, query, activeCategory])

  return (
    <div className="view">
      <div className="view-head">
        <div>
          <h1 className="view-title">Catalog</h1>
          <p className="view-sub">
            Browse {connectors.length} connectors and mount a source.
          </p>
        </div>
      </div>

      {loading && <div className="placeholder">Loading connectors…</div>}
      {error && <div className="banner banner-error">{error}</div>}
      {!loading && !error && connectors.length === 0 && (
        <div className="placeholder">No connectors available.</div>
      )}

      {!loading && !error && connectors.length > 0 && (
        <>
          <div className="toolbar">
            <div className="search">
              <SearchIcon size={16} className="search-icon" />
              <input
                className="search-input"
                type="search"
                placeholder="Search connectors…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                aria-label="Search connectors"
              />
            </div>
            <span className="toolbar-count">
              {filtered.length} of {connectors.length}
            </span>
          </div>

          <div className="pills">
            <button
              type="button"
              className={`pill${activeCategory === ALL ? ' pill-active' : ''}`}
              onClick={() => setActiveCategory(ALL)}
              style={
                activeCategory === ALL
                  ? { background: 'var(--text)', borderColor: 'var(--text)', color: '#fff' }
                  : undefined
              }
            >
              All
              <span className="pill-count">{connectors.length}</span>
            </button>
            {visibleCategories.map((category) => {
              const active = activeCategory === category
              const color = categoryColor(category)
              return (
                <button
                  key={category}
                  type="button"
                  className={`pill${active ? ' pill-active' : ''}`}
                  onClick={() => setActiveCategory(category)}
                  style={
                    active
                      ? { background: color, borderColor: color, color: '#fff' }
                      : { borderColor: 'var(--border)' }
                  }
                >
                  {!active && <span className="pill-dot" style={{ background: color }} />}
                  {category}
                  <span className="pill-count">{categoryCounts.get(category)}</span>
                </button>
              )
            })}
          </div>

          {filtered.length === 0 ? (
            <div className="placeholder">No connectors match your search.</div>
          ) : (
            <div className="card-grid">
              {filtered.map(({ connector, category, description }) => {
                const color = categoryColor(category)
                const site = websiteFor(connector.name)
                return (
                  <button
                    key={connector.name}
                    type="button"
                    className="conn-card"
                    onClick={() => onOpen(connector)}
                  >
                    <div className="conn-card-top">
                      <BrandLogo name={connector.name} size={44} />
                      <span
                        className="conn-badge"
                        style={{ color, background: `${color}18` }}
                      >
                        {category}
                      </span>
                      {site && (
                        <a
                          className="conn-link"
                          href={site}
                          target="_blank"
                          rel="noopener noreferrer"
                          onClick={(e) => e.stopPropagation()}
                          aria-label={`Open ${displayName(connector.name)} website`}
                          title="Open website"
                        >
                          <ExternalIcon size={15} />
                        </a>
                      )}
                    </div>
                    <div className="conn-name">{displayName(connector.name)}</div>
                    <p className="conn-desc">{description}</p>
                    <span className="conn-connect">Connect →</span>
                  </button>
                )
              })}
            </div>
          )}
        </>
      )}
    </div>
  )
}
