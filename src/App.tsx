import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { Group, LogLine, Server, ServerView, Status, Stray, Worktree } from './api'
import { api, blankServer, isLive, onEvent, parentFolder } from './api'
import Rail from './ui/Rail'
import type { Route } from './ui/Rail'
import Home from './ui/Home'
import Detail from './ui/Detail'
import Editor from './ui/Editor'
import Settings from './ui/Settings'
import Strays from './ui/Strays'
import Intro from './ui/Intro'

/** Matches the supervisor's ring buffer, so scrollback stays bounded. */
const MAX_LINES = 2000

/** Opening: the icon, then the sun leaving it for the hero, then the app.
 *  `holding` also covers the first read, so nothing is shown half-filled. */
type Launch = 'holding' | 'arriving' | 'done'

export default function App() {
  const [views, setViews] = useState<ServerView[]>([])
  const [groups, setGroups] = useState<Group[]>([])
  // Worktrees are read once for every server here rather than per component:
  // the cards each want the current branch, and Worktrees.tsx wants the whole
  // list, so fetching in either place would mean one call per card per render.
  const [trees, setTrees] = useState<Map<string, Worktree[]>>(new Map())
  // The design harness can open straight onto a screen (preview.html?at=…),
  // so each one can be reviewed without clicking through. Undefined in the
  // real app, which always opens on the index.
  const [route, setRoute] = useState<Route>(
    () => (window as unknown as { __cucinaRoute?: Route }).__cucinaRoute ?? { kind: 'all' },
  )
  const [lines, setLines] = useState<LogLine[]>([])
  const [editing, setEditing] = useState<Server | null>(
    // The harness can also open straight into the sheet (preview.html?at=add).
    () => ((window as unknown as { __cucinaAdd?: boolean }).__cucinaAdd ? blankServer() : null),
  )
  const [home, setHome] = useState('')
  const [now, setNow] = useState(() => Date.now())
  // Strays are observed, never stored: the list, when it was taken, and what
  // went wrong if the probe failed. Held here rather than in the page, because
  // the rail shows the count whether or not the page is open.
  const [strays, setStrays] = useState<Stray[]>([])
  const [scannedAt, setScannedAt] = useState<number | null>(null)
  const [scanning, setScanning] = useState(false)
  const [scanError, setScanError] = useState<string | null>(null)
  const [adopting, setAdopting] = useState<string | null>(null)
  // Someone who has asked for less motion has asked for this most of all, so
  // for them the app simply opens.
  const [launch, setLaunch] = useState<Launch>(() =>
    window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'done' : 'holding',
  )
  /** The first list is in, so the hero exists for the sun to aim at. */
  const [ready, setReady] = useState(false)
  const disc = useRef<HTMLSpanElement>(null)

  // The listener registers once and reads the open server through a ref,
  // rather than re-subscribing on every navigation.
  const open = route.kind === 'server' ? route.id : null
  const openRef = useRef<string | null>(null)
  openRef.current = open

  const refresh = useCallback(async () => {
    const [nextViews, nextGroups] = await Promise.all([api.list(), api.groups()])
    setViews(nextViews)
    setGroups(nextGroups)

    // A directory that isn't a git repo just yields an empty list, so a
    // failure here should never take the rest of the refresh down with it.
    const found = await Promise.all(
      nextViews.map(async (v) => {
        const list = await api.worktrees(v.server.id).catch(() => [] as Worktree[])
        return [v.server.id, list] as const
      }),
    )
    setTrees(new Map(found))
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
    // Ready either way: a read that failed is still an answer, and the sun
    // should not be left sitting on the horizon over it.
    void refresh().catch(console.error).finally(() => setReady(true))
    void api.homeDir().then(setHome)
  }, [refresh])

  // Reading the process table is cheap but not free, and a list of processes
  // goes stale the moment it is taken — so it is read when the window comes
  // forward and when Scan is pressed, and on no timer whatsoever.
  //
  // `loud` draws the scanning state. A scan the user asked for should show its
  // work; one that follows a stop, or comes with the window, should not throw
  // the list they are reading away and put skeletons in its place.
  const scan = useCallback(async (loud = false) => {
    if (loud) setScanning(true)
    try {
      setStrays(await api.strays())
      setScannedAt(Date.now())
      setScanError(null)
    } catch (e) {
      // The last good list and its timestamp both stay: an empty list would
      // read as good news, which is the opposite of what happened.
      setScanError(String(e))
    } finally {
      setScanning(false)
    }
  }, [])

  useEffect(() => {
    void scan(true)
    const quiet = () => void scan()
    window.addEventListener('focus', quiet)
    return () => window.removeEventListener('focus', quiet)
  }, [scan])

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

  const addServer = useCallback(() => {
    setAdopting(null)
    setEditing(blankServer())
  }, [])

  /** Adopt opens the Add sheet already filled in. The stray keeps running —
      Cucina does not own it until it is started from here. */
  const adopt = useCallback((stray: Stray) => {
    const dir = stray.dir ?? ''
    setAdopting(stray.command)
    setEditing({
      ...blankServer(),
      name: dir.split('/').filter(Boolean).pop() ?? '',
      dir,
      command: stray.command,
      group: parentFolder(dir),
    })
  }, [])

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


  const openPort = useCallback((port: number) => {
    void api.openUrl(`http://localhost:${port}`).catch(console.error)
  }, [])

  // Both memoised: Intro watches them, and a new identity each render would
  // restart an effect that is only allowed to run once.
  const arrive = useCallback(() => setLaunch('arriving'), [])
  const opened = useCallback(() => setLaunch('done'), [])

  return (
    <>
    <div className={`app${launch === 'done' ? '' : ` ${launch}`}`}>
      <Rail
        route={route}
        groups={groups}
        views={views}
        strays={strays.length}
        onRoute={setRoute}
        onAdd={addServer}
      />

      <main className="page">
        <div className="page-drag" data-tauri-drag-region />

        {route.kind === 'settings' ? (
          <Settings onScanStrays={() => setRoute({ kind: 'strays' })} />
        ) : route.kind === 'strays' ? (
          <Strays
            strays={strays}
            at={scannedAt}
            scanning={scanning}
            error={scanError}
            home={home}
            onScan={() => void scan(true)}
            onStop={async (pid) => {
              await api.stopStray(pid)
              // The row leaves and the count on the rail drops, both because
              // the list is read again rather than edited in place.
              await scan()
            }}
            onAdopt={adopt}
          />
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
            trees={trees.get(current.server.id) ?? []}
            onSwitched={() => void refresh()}
          />
        ) : (
          <Home
            views={views}
            groups={groups}
            disc={disc}
            trees={trees}
            project={route.kind === 'project' ? route.name : null}
            now={now}
            onOpen={(id) => setRoute({ kind: 'server', id })}
            onStart={start}
            onStop={stop}
            onAdd={addServer}
            onGroupRun={onGroupRun}
            onMove={onMove}
            onOpenPort={openPort}
          />
        )}
      </main>

      {editing ? (
        <Editor
          server={editing}
          home={home}
          groups={groupNames}
          observed={adopting ?? undefined}
          onSaved={(saved) => {
            setEditing(null)
            setAdopting(null)
            void refresh().then(() => setRoute({ kind: 'server', id: saved.id }))
          }}
          onDeleted={() => {
            setEditing(null)
            setAdopting(null)
            setRoute({ kind: 'all' })
            void refresh()
          }}
          onCancel={() => {
            setEditing(null)
            setAdopting(null)
          }}
        />
      ) : null}
    </div>

    {/* Outside .app, which is held at nothing until the sun has gone. */}
    {launch === 'done' ? null : (
      <Intro ready={ready} disc={disc} onSettle={arrive} onDone={opened} />
    )}
    </>
  )
}
