import { SquaresFour, GearSix, Plus } from '@phosphor-icons/react'
import type { Group, ServerView } from '../api'
import { isLive } from '../api'

export type Route =
  | { kind: 'all' }
  | { kind: 'project'; name: string }
  | { kind: 'server'; id: string }
  | { kind: 'strays' }
  | { kind: 'settings' }

interface Props {
  route: Route
  groups: Group[]
  views: ServerView[]
  /** How many processes are holding a port that Cucina does not own. */
  strays: number
  onRoute: (route: Route) => void
  onAdd: () => void
}

/** Projects are marked by their initial — set in the same Helvetica as
    everything else, so the rail carries no stray artwork. */
const initial = (name: string) => name.trim().charAt(0).toUpperCase() || '·'

/* Home's tidy 2×2 grid, with one square broken out of it. The two sit adjacent
   in the rail, and it is that adjacency that carries the meaning — so the
   square here is Phosphor's own `squares-four` square, to the unit: an 84-wide
   outline with a 24 stroke and a 20 outer radius, which a stroked 60-box at
   inset 12 reproduces exactly. Drawing a different square would make the pair
   read as two icons from two sets rather than one idea and its exception.

   Only the pitch is ours, tightened from 100 to 90 so the loose square has
   somewhere to go inside the same viewBox. It stays at full strength while the
   grid drops back — flatten them to one weight and this is a meaningless
   four-square. */
const BOX = 256
/** Phosphor's own square: 84 across the outside, 20 of corner. The stroke is a
    hair under its 24 — the loose one sits white on ink, where the same weight
    optically bolds against the outlined trio beside it. */
const OUTER = 84
const RADIUS = 20
const STROKE = 21
const PITCH = 90
/** How far the loose one has left its slot — a third of a pitch. Enough to
    read as deliberate at 15px, not so far that it stops belonging to the grid
    it broke out of. */
const BREAK = 30
/** Derived, so the mark stays centred in its tile whatever the pitch and the
    break are set to. The loose square is what makes this wider than Home. */
const SPAN = PITCH + BREAK + OUTER
const ORIGIN = (BOX - SPAN) / 2

function Square({ at, dim }: { at: [number, number]; dim?: boolean }) {
  return (
    <rect
      // A stroke straddles its path, so the path is inset by half of it and
      // the corner radius comes in by the same amount. That is what makes the
      // outside land on 84 and 20 exactly.
      x={at[0] + STROKE / 2}
      y={at[1] + STROKE / 2}
      width={OUTER - STROKE}
      height={OUTER - STROKE}
      rx={RADIUS - STROKE / 2}
      fill="none"
      stroke="currentColor"
      strokeOpacity={dim ? 0.45 : 1}
      strokeWidth={STROKE}
    />
  )
}

function StraysMark() {
  const a = ORIGIN
  const b = ORIGIN + PITCH
  return (
    <svg className="stray-mark" viewBox={`0 0 ${BOX} ${BOX}`} aria-hidden focusable="false">
      <Square at={[a, a]} dim />
      <Square at={[b, a]} dim />
      <Square at={[a, b]} dim />
      <Square at={[b + BREAK, b + BREAK]} />
    </svg>
  )
}

export default function Rail({ route, groups, views, strays, onRoute, onAdd }: Props) {
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

      {/* Absent at zero, which is most days — nothing holds a slot open for an
          empty page. Settings keeps a way in for when the count is nothing. */}
      {strays > 0 ? (
        <button
          className={`rail-btn${route.kind === 'strays' ? ' on' : ''}`}
          onClick={() => onRoute({ kind: 'strays' })}
          title={`${strays} stray${strays === 1 ? '' : 's'}`}
          aria-label={`${strays} stray${strays === 1 ? '' : 's'}`}
        >
          <StraysMark />
          <span className="rail-badge">{strays}</span>
        </button>
      ) : null}

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
