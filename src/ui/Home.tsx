import { useEffect, useRef, useState } from 'react'
import type { PointerEvent as ReactPointerEvent } from 'react'
import type { Group, ServerView } from '../api'
import { isLive, sections } from '../api'
import ServerCard from './ServerCard'
import bigMark from '../../assets/mark-160.png'


/** A small, deliberately Italian set — enough to tell projects apart at a
    glance without becoming a full emoji picker. */
const ICONS = [
  '🍅', '🌿', '🍋', '🫒', '🍇', '🌞',
  '🐙', '🍝', '🥖', '🧀', '☕️', '🍷',
  '🏛', '🌊', '🔥', '🪴', '⚙️', '🧭',
  '📦', '🛰', '🧪', '🎛', '🗄', '🔭',
]

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
  /** null shows everything grouped; a name shows just that project. */
  project: string | null
  now: number
  onOpen: (id: string) => void
  onStart: (id: string) => void
  onStop: (id: string) => void
  onAdd: () => void
  onGroupRun: (group: string, start: boolean) => void
  onMove: (serverId: string, group: string) => void
  onIcon: (group: string, icon: string) => void
  onOpenPort: (port: number) => void
}

export default function Home({
  views,
  groups,
  project,
  now,
  onOpen,
  onStart,
  onStop,
  onAdd,
  onGroupRun,
  onMove,
  onIcon,
  onOpenPort,
}: Props) {
  const [drag, setDrag] = useState<Drag | null>(null)
  const [picker, setPicker] = useState<string | null>(null)

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

  useEffect(() => {
    if (!picker) return
    const close = (e: MouseEvent) => {
      if (!(e.target as HTMLElement).closest('.icon-pop, .section-icon')) setPicker(null)
    }
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && setPicker(null)
    document.addEventListener('mousedown', close)
    document.addEventListener('keydown', esc)
    return () => {
      document.removeEventListener('mousedown', close)
      document.removeEventListener('keydown', esc)
    }
  }, [picker])

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
  const list = project === null
    ? sections(views)
    : [{ name: project, views: scoped }]
  const running = scoped.filter(isLive)
  const ports = running.map((v) => v.status.port).filter((p): p is number => Boolean(p))
  const dragged = drag && views.find((v) => v.server.id === drag.id)
  const showLoose = list.some((s) => s.name === '') || Boolean(dragged?.server.group)

  if (scoped.length === 0 && project !== null) {
    return (
      <div className="page-scroll">
        <div className="page-head">
          <h1>{project}</h1>
        </div>
        <div className="empty">
          <img src={bigMark} alt="" draggable={false} />
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
        <div className="page-head">
          <h1>Cucina</h1>
        </div>
        <div className="empty">
          <img src={bigMark} alt="" draggable={false} />
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
    const icon = groups.find((g) => g.name === name)?.icon ?? ''
    return (
      <section
        key={name || ' loose'}
        ref={zoneRef(name)}
        className={`section${drag?.over === name ? ' drop-over' : ''}`}
      >
        <header className="section-head">
          {name ? (
            <button
              className={`section-icon${icon ? '' : ' unset'}`}
              title={icon ? 'Change icon' : 'Give this project an icon'}
              onClick={() => setPicker(picker === name ? null : name)}
            >
              {icon || '+'}
            </button>
          ) : null}
          <span className="section-name">{name || 'No project'}</span>
          <span className="section-pips" title={`${live} of ${items.length} running`}>
            {items.slice(0, 8).map((v, i) => (
              <i key={v.server.id} className={i < live ? 'lit' : undefined} />
            ))}
            {items.length > 8 ? <span className="more">+{items.length - 8}</span> : null}
          </span>
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

        {picker === name ? (
          <div className="icon-pop">
            <div className="icon-grid">
              {ICONS.map((glyph) => (
                <button
                  key={glyph}
                  className={`icon-choice${glyph === icon ? ' current' : ''}`}
                  onClick={() => {
                    onIcon(name, glyph === icon ? '' : glyph)
                    setPicker(null)
                  }}
                >
                  {glyph}
                </button>
              ))}
            </div>
          </div>
        ) : null}

        <div className="grid">
          {items.map((view) => (
            <ServerCard
              key={view.server.id}
              view={view}
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
      <div className="page-head">
        {project ? (
          <>
            <span className="page-emoji">
              {groups.find((g) => g.name === project)?.icon ?? ''}
            </span>
            <h1>{project}</h1>
          </>
        ) : (
          <>
            <img className="page-mark" src={bigMark} alt="" draggable={false} />
            <h1>Cucina</h1>
          </>
        )}
        <span className="sub">
          {scoped.length} server{scoped.length === 1 ? '' : 's'}
        </span>
      </div>

      <div className={`hero${running.length ? ' warm' : ''}`}>
        <div className="hero-body">
          <h2 className="hero-count">
            {running.length ? `${running.length} running` : 'Nothing running'}
          </h2>
          {ports.length ? (
            <div className="hero-ports">
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
            </div>
          ) : (
            <p className="hero-caption">
              Everything is off. Start a server here, from the menu bar, or by handing it to a
              coding agent.
            </p>
          )}
        </div>
        {running.length ? (
          <button
            className="btn"
            onClick={() => running.forEach((v) => onStop(v.server.id))}
            title="Stop every running server"
          >
            Stop all
          </button>
        ) : null}
      </div>

      {project !== null ? (
        <div className="grid">
          {scoped.map((view) => (
            <ServerCard
              key={view.server.id}
              view={view}
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
