import { useRef, useState } from 'react'
import type { PointerEvent as ReactPointerEvent, RefObject } from 'react'
import type { Group, ServerView, Worktree } from '../api'
import { isLive, sections, spell } from '../api'
import ServerCard from './ServerCard'
import mark from '../../assets/cucina-mark.svg'

/** How far the pointer must travel before a press becomes a drag. */
const THRESHOLD = 6

interface Drag {
  id: string
  name: string
  x: number
  y: number
  over: string | null
}

interface Props {
  views: ServerView[]
  groups: Group[]
  /** Read once in App, so a card can show its branch without fetching. */
  trees: Map<string, Worktree[]>
  /** The launch animation lands its sun on this, so it has to be measurable
      from outside. Null on the empty states, which have no hero. */
  disc: RefObject<HTMLSpanElement | null>
  /** null shows everything grouped; a name shows just that project. */
  project: string | null
  now: number
  onOpen: (id: string) => void
  onStart: (id: string) => void
  onStop: (id: string) => void
  onAdd: () => void
  onGroupRun: (group: string, start: boolean) => void
  onMove: (serverId: string, group: string) => void
  onOpenPort: (port: number) => void
}

export default function Home({
  views,
  trees,
  disc,
  project,
  now,
  onOpen,
  onStart,
  onStop,
  onAdd,
  onGroupRun,
  onMove,
  onOpenPort,
}: Props) {
  const [drag, setDrag] = useState<Drag | null>(null)

  const pending = useRef<{ id: string; name: string; x: number; y: number } | null>(null)
  const zones = useRef(new Map<string, HTMLElement>())

  // The drag also lives in a ref: pointerup can arrive in the same tick as the
  // pointermove that started it, before React re-renders, and reading state
  // there would see null and mistake a fast drag for a click.
  const dragRef = useRef<Drag | null>(null)
  const track = (next: Drag | null) => {
    dragRef.current = next
    setDrag(next)
  }

  const zoneAt = (x: number, y: number): string | null => {
    for (const [name, el] of zones.current) {
      const r = el.getBoundingClientRect()
      if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return name
    }
    return null
  }

  const zoneRef = (name: string) => (el: HTMLElement | null) => {
    if (el) zones.current.set(name, el)
    else zones.current.delete(name)
  }

  /** Only shown when there is more than one worktree to be in — the same
      guard the switcher itself applies. */
  const branchOf = (id: string): string | null => {
    const list = trees.get(id) ?? []
    if (list.length < 2) return null
    return list.find((w) => w.isCurrent)?.branch ?? null
  }

  const handlers = (view: ServerView) => ({
    onPointerDown: (e: ReactPointerEvent) => {
      if (e.button !== 0) return
      pending.current = {
        id: view.server.id,
        name: view.server.name,
        x: e.clientX,
        y: e.clientY,
      }
    },
    onPointerMove: (e: ReactPointerEvent) => {
      const active = dragRef.current
      const start = pending.current
      if (!active && start) {
        if (Math.hypot(e.clientX - start.x, e.clientY - start.y) < THRESHOLD) return
        e.currentTarget.setPointerCapture(e.pointerId)
        track({ ...start, x: e.clientX, y: e.clientY, over: zoneAt(e.clientX, e.clientY) })
        return
      }
      if (active) {
        track({ ...active, x: e.clientX, y: e.clientY, over: zoneAt(e.clientX, e.clientY) })
      }
    },
    onPointerUp: () => {
      const active = dragRef.current
      if (active) {
        const from = views.find((v) => v.server.id === active.id)?.server.group ?? ''
        if (active.over !== null && active.over !== from) onMove(active.id, active.over)
        track(null)
      } else if (pending.current) {
        onOpen(pending.current.id)
      }
      pending.current = null
    },
    onPointerCancel: () => {
      track(null)
      pending.current = null
    },
  })

  const scoped = project === null ? views : views.filter((v) => v.server.group === project)
  const list = project === null ? sections(views) : [{ name: project, views: scoped }]
  const running = scoped.filter(isLive)
  const ports = running.map((v) => v.status.port).filter((p): p is number => Boolean(p))
  const dragged = drag && views.find((v) => v.server.id === drag.id)
  const showLoose = list.some((s) => s.name === '') || Boolean(dragged?.server.group)

  // Name, then a rule of empty space, then the one action. With the mark gone
  // the wordmark starts on the same content margin as the hero and the grid
  // below it, which is most of why the header used to feel bolted on.
  const head = (
    <div className="page-head">
      <h1>{project ?? 'Cucina'}</h1>
      <span className="spacer" />
      {running.length ? (
        <button
          className="head-action"
          onClick={() => running.forEach((v) => onStop(v.server.id))}
          title="Stop every running server"
        >
          Stop all
        </button>
      ) : null}
    </div>
  )

  if (scoped.length === 0 && project !== null) {
    return (
      <div className="page-scroll">
        {head}
        <div className="empty">
          <img src={mark} alt="" draggable={false} />
          <h2>This project is empty</h2>
          <p>Add a server to it, or drag one in from All servers.</p>
          <button className="btn primary" onClick={onAdd}>
            Add a server
          </button>
        </div>
      </div>
    )
  }

  if (views.length === 0) {
    return (
      <div className="page-scroll">
        {head}
        <div className="empty">
          <img src={mark} alt="" draggable={false} />
          <h2>Nothing on the heat</h2>
          <p>
            Point Cucina at a directory and a command. Starting that server then takes one click —
            from here, from the menu bar, or from a coding agent.
          </p>
          <button className="btn primary" onClick={onAdd}>
            Add a server
          </button>
        </div>
      </div>
    )
  }

  const section = (name: string, items: ServerView[]) => {
    const live = items.filter(isLive).length
    const allLive = live === items.length
    return (
      <section
        key={name || ' loose'}
        ref={zoneRef(name)}
        className={`section${drag?.over === name ? ' drop-over' : ''}`}
      >
        {/* Label then a rule to the far margin — it says where the group ends
            without putting another badge on the page. */}
        <header className="section-head">
          <span className="section-name">{name || 'No project'}</span>
          <span className="section-rule" />
          {name && items.length ? (
            <button
              className="section-action"
              onClick={() => onGroupRun(name, !allLive)}
              title={`${allLive ? 'Stop' : 'Start'} every server in ${name}`}
            >
              {allLive ? 'Stop all' : 'Start all'}
            </button>
          ) : null}
        </header>

        <div className="grid">
          {items.map((view) => (
            <ServerCard
              key={view.server.id}
              view={view}
              branch={branchOf(view.server.id)}
              now={now}
              lifting={drag?.id === view.server.id}
              onOpen={() => onOpen(view.server.id)}
              onStart={() => onStart(view.server.id)}
              onStop={() => onStop(view.server.id)}
              {...handlers(view)}
            />
          ))}
        </div>

        {drag && !items.length ? (
          <p className="drop-note">Drop here to take it out of its project</p>
        ) : null}
      </section>
    )
  }

  return (
    <div className="page-scroll">
      {head}

      <div className="hero">
        {/* A field of concentric hairlines, and the one solid disc in the app
            sitting off-centre inside it. */}
        <span className="hero-field" aria-hidden />
        <span className="hero-disc" ref={disc} aria-hidden />

        <div className="hero-body">
          <h2 className="hero-count">{running.length}</h2>
          <span className="hero-word">running</span>
        </div>

        <div className="hero-foot">
          {ports.map((port) => (
            <button
              key={port}
              className="port-pill"
              onClick={() => onOpenPort(port)}
              title={`Open localhost:${port}`}
            >
              :{port}
            </button>
          ))}
          <p className="hero-caption">
            of {spell(scoped.length)} server{scoped.length === 1 ? '' : 's'}
          </p>
        </div>
      </div>

      {project !== null ? (
        <div className="grid">
          {scoped.map((view) => (
            <ServerCard
              key={view.server.id}
              view={view}
              branch={branchOf(view.server.id)}
              now={now}
              lifting={false}
              onOpen={() => onOpen(view.server.id)}
              onStart={() => onStart(view.server.id)}
              onStop={() => onStop(view.server.id)}
              {...handlers(view)}
            />
          ))}
        </div>
      ) : (
        <>
          {list.filter((s) => s.name).map((s) => section(s.name, s.views))}
          {showLoose ? section('', list.find((s) => s.name === '')?.views ?? []) : null}
        </>
      )}

      {drag ? (
        <div className="ghost" style={{ left: drag.x, top: drag.y }}>
          {drag.name}
        </div>
      ) : null}
    </div>
  )
}
