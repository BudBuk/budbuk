import { useCallback, useEffect, useRef, useState } from 'react'
import { NavLink, Navigate, Route, Routes, useNavigate } from 'react-router-dom'
import { getConnectors, getSources, type Connector, type Source } from './api'
import Analytics from './components/Analytics'
import Catalog from './components/Catalog'
import MountModal from './components/MountModal'
import SourcesView from './components/SourcesView'
import { CatalogIcon, ChartIcon, Logo, SourcesIcon } from './components/icons'

const POLL_MS = 5000

const NAV: { to: string; label: string; icon: (p: { size?: number }) => JSX.Element }[] = [
  { to: '/catalog', label: 'Catalog', icon: CatalogIcon },
  { to: '/sources', label: 'Sources', icon: SourcesIcon },
  { to: '/analytics', label: 'Analytics', icon: ChartIcon },
]

export default function App() {
  const navigate = useNavigate()
  const [mounting, setMounting] = useState<Connector | null>(null)

  const [connectors, setConnectors] = useState<Connector[]>([])
  const [connectorsLoading, setConnectorsLoading] = useState(true)
  const [connectorsError, setConnectorsError] = useState<string | null>(null)

  const [sources, setSources] = useState<Source[]>([])
  const [sourcesLoading, setSourcesLoading] = useState(true)
  const [sourcesError, setSourcesError] = useState<string | null>(null)

  const loadConnectors = useCallback(async () => {
    setConnectorsLoading(true)
    setConnectorsError(null)
    try {
      setConnectors(await getConnectors())
    } catch (e) {
      setConnectorsError(e instanceof Error ? e.message : 'Failed to load')
    } finally {
      setConnectorsLoading(false)
    }
  }, [])

  const loadSources = useCallback(async () => {
    try {
      setSources(await getSources())
      setSourcesError(null)
    } catch (e) {
      setSourcesError(e instanceof Error ? e.message : 'Failed to load sources')
    } finally {
      setSourcesLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadConnectors()
    void loadSources()
  }, [loadConnectors, loadSources])

  // Poll sources every ~5s to keep status live.
  const loadSourcesRef = useRef(loadSources)
  loadSourcesRef.current = loadSources
  useEffect(() => {
    const id = window.setInterval(() => void loadSourcesRef.current(), POLL_MS)
    return () => window.clearInterval(id)
  }, [])

  function handleMounted() {
    setMounting(null)
    navigate('/sources')
    void loadSources()
  }

  return (
    <div className="app">
      <header className="topbar">
        <div className="topbar-brand">
          <Logo size={30} />
          <span className="topbar-name">BudBuk</span>
          <span className="topbar-tag">Control Panel</span>
        </div>
        <div className="topbar-status">
          <span className="status-pill">
            <span className="status-pill-dot" />
            {sources.length} source{sources.length === 1 ? '' : 's'}
          </span>
        </div>
      </header>

      <div className="layout">
        <nav className="sidebar">
          <div className="nav-section-label">Workspace</div>
          {NAV.map(({ to, label, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              className={({ isActive }) => `nav-item${isActive ? ' nav-item-active' : ''}`}
            >
              <Icon size={18} />
              <span>{label}</span>
              {to === '/sources' && sources.length > 0 && (
                <span className="nav-badge">{sources.length}</span>
              )}
            </NavLink>
          ))}
          <div className="sidebar-foot">BudBuk · local control panel</div>
        </nav>

        <main className="main">
          <Routes>
            <Route path="/" element={<Navigate to="/catalog" replace />} />
            <Route
              path="/catalog"
              element={
                <Catalog
                  connectors={connectors}
                  loading={connectorsLoading}
                  error={connectorsError}
                  onOpen={setMounting}
                />
              }
            />
            <Route
              path="/sources"
              element={
                <SourcesView
                  sources={sources}
                  loading={sourcesLoading}
                  error={sourcesError}
                  onChanged={loadSources}
                  onGoToCatalog={() => navigate('/catalog')}
                />
              }
            />
            <Route
              path="/analytics"
              element={<Analytics sources={sources} loading={sourcesLoading} error={sourcesError} />}
            />
            <Route path="*" element={<Navigate to="/catalog" replace />} />
          </Routes>
        </main>
      </div>

      {mounting && (
        <MountModal
          connector={mounting}
          onClose={() => setMounting(null)}
          onMounted={handleMounted}
        />
      )}
    </div>
  )
}
