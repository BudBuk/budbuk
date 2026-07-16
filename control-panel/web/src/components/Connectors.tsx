interface Props {
  connectors: string[]
  selected: string | null
  loading: boolean
  error: string | null
  onSelect: (name: string) => void
}

export default function Connectors({
  connectors,
  selected,
  loading,
  error,
  onSelect,
}: Props) {
  return (
    <section className="panel">
      <h2>Connectors</h2>
      {loading && <p className="muted">Loading connectors…</p>}
      {error && <p className="error">{error}</p>}
      {!loading && !error && connectors.length === 0 && (
        <p className="muted">No connectors available.</p>
      )}
      <div className="chips">
        {connectors.map((name) => (
          <button
            key={name}
            className={`chip${selected === name ? ' chip-selected' : ''}`}
            onClick={() => onSelect(name)}
            type="button"
          >
            {name}
          </button>
        ))}
      </div>
    </section>
  )
}
