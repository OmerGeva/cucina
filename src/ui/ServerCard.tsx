import type { CSSProperties, PointerEvent as ReactPointerEvent } from 'react'
import { Play, Stop } from '@phosphor-icons/react'
import type { ServerView } from '../api'
import { originLabel, uptime } from '../api'
import { tileUrl } from '../tiles'

interface Props {
  view: ServerView
  now: number
  lifting: boolean
  onOpen: () => void
  onStart: () => void
  onStop: () => void
  onPointerDown: (e: ReactPointerEvent) => void
  onPointerMove: (e: ReactPointerEvent) => void
  onPointerUp: () => void
  onPointerCancel: () => void
}

export default function ServerCard({
  view,
  now,
  lifting,
  onOpen,
  onStart,
  onStop,
  onPointerDown,
  onPointerMove,
  onPointerUp,
  onPointerCancel,
}: Props) {
  const { server, status } = view
  const live = status.state === 'running' || status.state === 'starting'
  const agent = originLabel(status.origin)

  // The command lives in the detail view. Here the useful facts are the name,
  // whether it's up, and the port — so state carries the hierarchy: a running
  // card is loud, an idle one gets out of the way.
  const caption = () => {
    if (status.state === 'crashed') {
      return status.exitCode != null ? `exited ${status.exitCode}` : 'crashed'
    }
    if (status.state === 'starting') return 'starting…'
    if (live && status.startedAt) {
      const up = uptime(status.startedAt, now)
      return agent ? `${up} · ${agent}` : up
    }
    return 'idle'
  }

  return (
    <div
      role="button"
      tabIndex={0}
      className={`card${live ? ' live' : ''}${status.state === 'crashed' ? ' broken' : ''}${
        lifting ? ' lifting' : ''
      }`}
      style={{ ['--ceramic' as string]: `url("${tileUrl(server)}")` } as CSSProperties}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerCancel}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          onOpen()
        }
      }}
    >
      <div className="card-top">
        <span className="card-name">{server.name}</span>
        <button
          className={`card-action ${live ? 'stop' : 'start'}`}
          aria-label={live ? `Stop ${server.name}` : `Start ${server.name}`}
          title={live ? 'Stop' : 'Start'}
          // The card opens the log view, so the control must not bubble to it.
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation()
            live ? onStop() : onStart()
          }}
        >
          {live ? <Stop weight="fill" /> : <Play weight="fill" />}
        </button>
      </div>

      <div className="card-body">
        {status.port ? (
          <span className="card-port">
            <span className="colon">:</span>
            {status.port}
          </span>
        ) : null}
        <div className={`card-meta${status.state === 'crashed' ? ' bad' : ''}`}>{caption()}</div>
      </div>
    </div>
  )
}
