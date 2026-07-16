import { useMemo, useState } from 'react'
import { type Connector } from '../api'
import { CATEGORIES, categoryColor, metaFor, monogram } from '../connectorMeta'

interface Props {
  connectors: Connector[]
  selected: string | null
  loading: boolean
  error: string | null
  onSelect: (name: string) => void
}

const ALL = '__all__'

export default function Connectors({
  connectors,
  selected,
  loading,
  error,
  onSelect,
}: Props) {
  const [query, setQuery] = useState('')
  const [activeCategory, setActiveCategory] = useState<string>(ALL)

  // Decorate each connector with its presentation metadata once.
  const decorated = useMemo(
    () =>
      connectors.map((c) => {
        const meta = metaFor(c.name)
        return { connector: c, ...meta }
      }),
    [connectors],
  )

  // Only show category chips for categories that actually have connectors,
  // and remember how many connectors live in each.
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
    <section className="panel">
      <h2>Connectors</h2>

      {loading && <p className="muted">Loading connectors…</p>}
      {error && <p className="error">{error}</p>}
      {!loading && !error && connectors.length === 0 && (
        <p className="muted">No connectors available.</p>
      )}

      {!loading && !error && connectors.length > 0 && (
        <>
          <div className="catalog-toolbar">
            <input
              className="input catalog-search"
              type="search"
              placeholder="Search connectors…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label="Search connectors"
            />
            <span className="muted small catalog-count">
              {filtered.length} of {connectors.length}
            </span>
          </div>

          <div className="filter-chips">
            <button
              type="button"
              className={`filter-chip${activeCategory === ALL ? ' filter-chip-active' : ''}`}
              onClick={() => setActiveCategory(ALL)}
            >
              All
              <span className="filter-chip-count">{connectors.length}</span>
            </button>
            {visibleCategories.map((category) => {
              const active = activeCategory === category
              return (
                <button
                  key={category}
                  type="button"
                  className={`filter-chip${active ? ' filter-chip-active' : ''}`}
                  onClick={() => setActiveCategory(category)}
                  style={
                    active
                      ? {
                          background: categoryColor(category),
                          borderColor: categoryColor(category),
                          color: '#fff',
                        }
                      : { borderColor: categoryColor(category) }
                  }
                >
                  <span
                    className="filter-chip-dot"
                    style={{ background: categoryColor(category) }}
                  />
                  {category}
                  <span className="filter-chip-count">
                    {categoryCounts.get(category)}
                  </span>
                </button>
              )
            })}
          </div>

          {filtered.length === 0 ? (
            <p className="muted">No connectors match your search.</p>
          ) : (
            <div className="connector-grid">
              {filtered.map(({ connector, category, description }) => {
                const isSelected = selected === connector.name
                const color = categoryColor(category)
                return (
                  <button
                    key={connector.name}
                    type="button"
                    className={`connector-card${isSelected ? ' connector-card-selected' : ''}`}
                    onClick={() => onSelect(connector.name)}
                    aria-pressed={isSelected}
                    style={
                      isSelected
                        ? {
                            borderColor: color,
                            boxShadow: `0 0 0 2px ${color} inset, 0 6px 16px rgba(15, 23, 42, 0.1)`,
                          }
                        : undefined
                    }
                  >
                    <div
                      className="connector-icon"
                      style={{ background: color }}
                      aria-hidden="true"
                    >
                      {monogram(connector.name)}
                    </div>
                    <div className="connector-body">
                      <div className="connector-name">{connector.name}</div>
                      <span
                        className="connector-badge"
                        style={{ background: color }}
                      >
                        {category}
                      </span>
                      <p className="connector-desc">{description}</p>
                    </div>
                  </button>
                )
              })}
            </div>
          )}
        </>
      )}
    </section>
  )
}
