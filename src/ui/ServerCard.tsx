import type { CSSProperties, PointerEvent as ReactPointerEvent } from 'react'
import { Play, Stop } from '@phosphor-icons/react'
import type { ServerView } from '../api'
import { originLabel, uptime } from '../api'
import { pitchFor } from '../rings'

interface Props {
  view: ServerView
  /** The worktree this server currently runs from, when there is more than
      one to be in. Null hides the line entirely, which is the normal case. */
  branch: string | null
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
  branch,
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
  // the branch, whether it's up, and the port — so state carries the
  // hierarchy: a running card is loud, an idle one gets out of the way.
  const caption = () => {
    if (status.state === 'crashed') {
      return status.exitCode != null ? `exited ${status.exitCode}` : 'crashed'
    }
    if (status.state === 'starting') return 'starting…'
    if (live && status.startedAt) {
      const up = `up ${uptime(status.startedAt, now)}`
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
      {live ? (
        <span
          className="card-rings"
          style={{ ['--pitch' as string]: pitchFor(server) } as CSSProperties}
        />
      ) : null}

      <div className="card-top">
        <div className="card-id">
          <span className="card-name">{server.name}</span>
          {branch ? (
            <span className="card-branch" title={branch}>
              <span className="glyph">⎇</span>
              <span className="name">{branch}</span>
            </span>
          ) : null}
        </div>

        <button
          className="card-action"
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
          <span className="card-port">{status.port}</span>
        ) : (
          <span className="card-dash" aria-hidden>
            —
          </span>
        )}
        <div className={`card-meta${status.state === 'crashed' ? ' bad' : ''}`}>{caption()}</div>
      </div>
    </div>
  )
}
