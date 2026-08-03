import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { Group, LogLine, Server, ServerView, Status } from './api'
import { api, blankServer, isLive, onEvent } from './api'
import Rail from './ui/Rail'
import type { Route } from './ui/Rail'
import Home from './ui/Home'
import Detail from './ui/Detail'
import Editor from './ui/Editor'
import Settings from './ui/Settings'

/** Matches the supervisor's ring buffer, so scrollback stays bounded. */
const MAX_LINES = 2000

export default function App() {
  const [views, setViews] = useState<ServerView[]>([])
  const [groups, setGroups] = useState<Group[]>([])
  const [route, setRoute] = useState<Route>({ kind: 'all' })
  const [lines, setLines] = useState<LogLine[]>([])
  const [editing, setEditing] = useState<Server | null>(null)
  const [home, setHome] = useState('')
  const [now, setNow] = useState(() => Date.now())

  // The listener registers once and reads the open server through a ref,
  // rather than re-subscribing on every navigation.
  const open = route.kind === 'server' ? route.id : null
  const openRef = useRef<string | null>(null)
  openRef.current = open

  const refresh = useCallback(async () => {
    const [nextViews, nextGroups] = await Promise.all([api.list(), api.groups()])
    setViews(nextViews)
    setGroups(nextGroups)
    setRoute((current) => {
      if (current.kind === 'server' && !nextViews.some((v) => v.server.id === current.id)) {
        return { kind: 'all' }
      }
      if (current.kind === 'project' && !nextGroups.some((g) => g.name === current.name)) {
        return { kind: 'all' }
      }
      return current
    })
  }, [])

  useEffect(() => {
    void refresh()
    void api.homeDir().then(setHome)
  }, [refresh])

  useEffect(() => {
    const pending = onEvent((event) => {
      if (event.type === 'serversChanged') {
        void refresh()
        return
      }
      if (event.type === 'status') {
        const { type: _type, ...status } = event
        setViews((prev) =>
          prev.map((v) => (v.server.id === status.id ? { ...v, status: status as Status } : v)),
        )
        return
      }
      if (event.type === 'log' && event.id === openRef.current) {
        setLines((prev) => {
          const merged = prev.concat(event.lines)
          return merged.length > MAX_LINES ? merged.slice(-MAX_LINES) : merged
        })
      }
    })
    return () => {
      void pending.then((unlisten) => unlisten())
    }
  }, [refresh])

  useEffect(() => {
    if (!open) {
      setLines([])
      return
    }
    let stale = false
    void api.logs(open).then((backlog) => {
      if (!stale) setLines(backlog)
    })
    return () => {
      stale = true
    }
  }, [open])

  const anyLive = views.some(isLive)

  // Uptime only needs to be roughly right, and only while something runs — so
  // there is no timer at all when nothing is on.
  useEffect(() => {
    if (!anyLive) return
    setNow(Date.now())
    const id = setInterval(() => setNow(Date.now()), 10_000)
    return () => clearInterval(id)
  }, [anyLive])

  const addServer = useCallback(() => setEditing(blankServer()), [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'n') {
        e.preventDefault()
        addServer()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [addServer])

  const current = useMemo(
    () => views.find((v) => v.server.id === open) ?? null,
    [views, open],
  )

  const groupNames = useMemo(() => groups.map((g) => g.name), [groups])

  const start = useCallback((id: string) => {
    void api.start(id).catch(console.error)
  }, [])

  const stop = useCallback((id: string) => {
    void api.stop(id).catch(console.error)
  }, [])

  const onGroupRun = useCallback(
    (group: string, begin: boolean) => {
      for (const view of views) {
        if (view.server.group !== group) continue
        if (begin && !isLive(view)) start(view.server.id)
        if (!begin && isLive(view)) stop(view.server.id)
      }
    },
    [views, start, stop],
  )

  /** Dragging a card between sections just rewrites its project. */
  const onMove = useCallback(
    (serverId: string, group: string) => {
      const view = views.find((v) => v.server.id === serverId)
      if (!view || view.server.group === group) return
      void api.save({ ...view.server, group }).catch(console.error)
    },
    [views],
  )

  const onIcon = useCallback((group: string, icon: string) => {
    void api.setGroupIcon(group, icon).catch(console.error)
  }, [])

  const openPort = useCallback((port: number) => {
    void api.openUrl(`http://localhost:${port}`).catch(console.error)
  }, [])

  return (
    <div className="app">
      <Rail
        route={route}
        groups={groups}
        views={views}
        onRoute={setRoute}
        onAdd={addServer}
      />

      <main className="page">
        <div className="page-drag" data-tauri-drag-region />

        {route.kind === 'settings' ? (
          <Settings />
        ) : current ? (
          <Detail
            view={current}
            lines={lines}
            home={home}
            now={now}
            onBack={() => setRoute({ kind: 'all' })}
            onStart={() => start(current.server.id)}
            onStop={() => stop(current.server.id)}
            onRestart={() => void api.restart(current.server.id).catch(console.error)}
            onEdit={() => setEditing(current.server)}
            onOpenPort={openPort}
            onReveal={() => void api.reveal(current.server.dir)}
            onClearLogs={() => {
              void api.clearLogs(current.server.id).then(() => setLines([]))
            }}
          />
        ) : (
          <Home
            views={views}
            groups={groups}
            project={route.kind === 'project' ? route.name : null}
            now={now}
            onOpen={(id) => setRoute({ kind: 'server', id })}
            onStart={start}
            onStop={stop}
            onAdd={addServer}
            onGroupRun={onGroupRun}
            onMove={onMove}
            onIcon={onIcon}
            onOpenPort={openPort}
          />
        )}
      </main>

      {editing ? (
        <Editor
          server={editing}
          home={home}
          groups={groupNames}
          onSaved={(saved) => {
            setEditing(null)
            void refresh().then(() => setRoute({ kind: 'server', id: saved.id }))
          }}
          onDeleted={() => {
            setEditing(null)
            setRoute({ kind: 'all' })
            void refresh()
          }}
          onCancel={() => setEditing(null)}
        />
      ) : null}
    </div>
  )
}
