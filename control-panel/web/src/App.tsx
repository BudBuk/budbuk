import { useCallback, useEffect, useRef, useState } from 'react'
import { getConnectors, getSources, type Source } from './api'
import Connectors from './components/Connectors'
import MountSource from './components/MountSource'
import Sources from './components/Sources'

const POLL_MS = 5000

export default function App() {
  const [connectors, setConnectors] = useState<string[]>([])
  const [connectorsLoading, setConnectorsLoading] = useState(true)
  const [connectorsError, setConnectorsError] = useState<string | null>(null)
  const [selected, setSelected] = useState<string | null>(null)

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

  // Initial load.
  useEffect(() => {
    void loadConnectors()
    void loadSources()
  }, [loadConnectors, loadSources])

  // Poll sources every ~5s. Keep the latest loader in a ref to avoid
  // re-creating the interval on each render.
  const loadSourcesRef = useRef(loadSources)
  loadSourcesRef.current = loadSources
  useEffect(() => {
    const id = window.setInterval(() => {
      void loadSourcesRef.current()
    }, POLL_MS)
    return () => window.clearInterval(id)
  }, [])

  return (
    <div className="app">
      <header className="app-header">
        <div className="brand">
          <span className="brand-mark">BB</span>
          <h1>BudBuk Control Panel</h1>
        </div>
      </header>

      <main className="content">
        <Connectors
          connectors={connectors}
          selected={selected}
          loading={connectorsLoading}
          error={connectorsError}
          onSelect={setSelected}
        />
        <MountSource
          selectedConnector={selected}
          onMounted={loadSources}
        />
        <Sources
          sources={sources}
          loading={sourcesLoading}
          error={sourcesError}
          onChanged={loadSources}
        />
      </main>
    </div>
  )
}
