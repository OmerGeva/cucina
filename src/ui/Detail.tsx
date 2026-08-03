import { useEffect, useRef } from 'react'
import { CaretLeft } from '@phosphor-icons/react'
import type { LogLine, ServerView } from '../api'
import { originLabel, shortenPath, uptime } from '../api'
import { Chip } from './bits'

interface Props {
  view: ServerView
  lines: LogLine[]
  home: string
  now: number
  onBack: () => void
  onStart: () => void
  onStop: () => void
  onRestart: () => void
  onEdit: () => void
  onOpenPort: (port: number) => void
  onReveal: () => void
  onClearLogs: () => void
}

export default function Detail({
  view,
  lines,
  home,
  now,
  onBack,
  onStart,
  onStop,
  onRestart,
  onEdit,
  onOpenPort,
  onReveal,
  onClearLogs,
}: Props) {
  const { server, status } = view
  const live = status.state === 'running' || status.state === 'starting'
  const agent = originLabel(status.origin)

  const scroll = useRef<HTMLDivElement>(null)
  // Follow the tail unless the user has scrolled up to read something.
  const stick = useRef(true)

  useEffect(() => {
    const el = scroll.current
    if (el && stick.current) el.scrollTop = el.scrollHeight
  }, [lines])

  useEffect(() => {
    stick.current = true
    const el = scroll.current
    if (el) el.scrollTop = el.scrollHeight
  }, [server.id])

  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && onBack()
    window.addEventListener('keydown', esc)
    return () => window.removeEventListener('keydown', esc)
  }, [onBack])

  const onScroll = () => {
    const el = scroll.current
    if (el) stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40
  }

  return (
    <>
      <header className="detail-head">
        <button className="back" onClick={onBack}>
          <CaretLeft weight="bold" size={13} />
          All servers
        </button>

        <div className="detail-title">
          <h1>{server.name}</h1>
          <Chip state={status.state} />
          {agent ? <span className="chip agent">by {agent}</span> : null}

          {status.port ? (
            <button
              className="detail-port"
              onClick={() => onOpenPort(status.port!)}
              title={`Open localhost:${status.port}`}
            >
              <span className="colon">:</span>
              {status.port}
            </button>
          ) : null}
        </div>

        <div className="detail-meta">
          <button className="path-btn" onClick={onReveal} title="Reveal in Finder">
            {shortenPath(server.dir, home)}
          </button>
          <span className="sep">·</span>
          <span>{server.command}</span>
          {live && status.startedAt ? (
            <>
              <span className="sep">·</span>
              <span>up {uptime(status.startedAt, now)}</span>
            </>
          ) : null}
          {status.state === 'crashed' && status.exitCode != null ? (
            <>
              <span className="sep">·</span>
              <span>exit {status.exitCode}</span>
            </>
          ) : null}
        </div>

        <div className="detail-tools">
          <button className="btn primary" onClick={live ? onStop : onStart}>
            {live ? 'Stop' : 'Start'}
          </button>
          <button className="btn" onClick={onRestart} disabled={!live}>
            Restart
          </button>
          <span className="spacer" />
          <button className="btn quiet small" onClick={onEdit}>
            Edit
          </button>
        </div>
      </header>

      <div className="well">
        <div className="well-head">
          <span className="well-title">Output</span>
          {lines.length ? (
            <span className="well-count">
              {lines.length} line{lines.length === 1 ? '' : 's'}
            </span>
          ) : null}
          <span className="spacer" />
          <button className="btn quiet small" onClick={onClearLogs} disabled={!lines.length}>
            Clear
          </button>
        </div>

        <div className="log-scroll" ref={scroll} onScroll={onScroll}>
          {lines.length === 0 ? (
            <p className="log-quiet">Nothing yet — output appears here once it starts.</p>
          ) : (
            lines.map((line) => (
              <div key={line.seq} className={`log-line ${line.stream}`}>
                {line.text || ' '}
              </div>
            ))
          )}
        </div>
      </div>
    </>
  )
}
