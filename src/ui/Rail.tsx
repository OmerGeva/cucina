import { SquaresFour, GearSix, Plus } from '@phosphor-icons/react'
import type { Group, ServerView } from '../api'
import { isLive } from '../api'

export type Route =
  | { kind: 'all' }
  | { kind: 'project'; name: string }
  | { kind: 'server'; id: string }
  | { kind: 'settings' }

interface Props {
  route: Route
  groups: Group[]
  views: ServerView[]
  onRoute: (route: Route) => void
  onAdd: () => void
}

/** Projects are marked by their initial — set in the same Helvetica as
    everything else, so the rail carries no stray artwork. */
const initial = (name: string) => name.trim().charAt(0).toUpperCase() || '·'

export default function Rail({ route, groups, views, onRoute, onAdd }: Props) {
  const liveIn = (name: string) => views.some((v) => v.server.group === name && isLive(v))

  return (
    <nav className="rail">
      {/* Tauri drives window dragging from this attribute; the Electron
          -webkit-app-region property does nothing in WKWebView. */}
      <div className="rail-top" data-tauri-drag-region />

      <button
        className={`rail-btn${route.kind === 'all' || route.kind === 'server' ? ' on' : ''}`}
        onClick={() => onRoute({ kind: 'all' })}
        title="All servers"
        aria-label="All servers"
      >
        <SquaresFour weight="bold" />
      </button>

      {groups.length ? <span className="rail-rule" /> : null}

      <div className="rail-projects">
        {groups.map((group) => (
          <button
            key={group.name}
            className={`rail-btn project${
              route.kind === 'project' && route.name === group.name ? ' on' : ''
            }`}
            onClick={() => onRoute({ kind: 'project', name: group.name })}
            title={group.name}
            aria-label={group.name}
          >
            <span className="rail-initial">{initial(group.name)}</span>
            {liveIn(group.name) ? <span className="rail-live" /> : null}
          </button>
        ))}
      </div>

      <span className="spacer" />

      <button className="rail-btn" onClick={onAdd} title="Add a server (⌘N)" aria-label="Add a server">
        <Plus weight="bold" />
      </button>
      <button
        className={`rail-btn${route.kind === 'settings' ? ' on' : ''}`}
        onClick={() => onRoute({ kind: 'settings' })}
        title="Settings"
        aria-label="Settings"
      >
        <GearSix weight="bold" />
      </button>
    </nav>
  )
}
