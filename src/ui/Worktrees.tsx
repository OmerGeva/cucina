import { useEffect, useRef, useState } from 'react'
import type { CSSProperties } from 'react'
import { CaretDown, Check } from '@phosphor-icons/react'
import type { Worktree } from '../api'
import { api } from '../api'

interface Props {
  serverId: string
  /** Re-read when the server's directory changes underneath us. */
  dir: string
  live: boolean
  onSwitched: () => void
}

/** Branch switcher. Hidden entirely when the directory isn't a git repository
    with more than one worktree — there is nothing to switch between. */
export default function Worktrees({ serverId, dir, live, onSwitched }: Props) {
  const [trees, setTrees] = useState<Worktree[]>([])
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  /** Fixed coordinates, clamped to the window — the trigger can sit anywhere
      along a path of any length, so a menu anchored to it will otherwise run
      off the edge. */
  const [box, setBox] = useState<CSSProperties>({})
  const root = useRef<HTMLDivElement>(null)
  const trigger = useRef<HTMLButtonElement>(null)

  const MENU_W = 300
  const EDGE = 10

  const place = () => {
    const r = trigger.current?.getBoundingClientRect()
    if (!r) return
    const left = Math.min(Math.max(EDGE, r.left), window.innerWidth - MENU_W - EDGE)
    const below = window.innerHeight - r.bottom - EDGE - 6
    const above = r.top - EDGE - 6
    // Prefer dropping down, but flip up when there is meaningfully more room.
    const drop = below >= 180 || below >= above
    setBox(
      drop
        ? { left, top: r.bottom + 6, maxHeight: Math.max(140, below) }
        : { left, bottom: window.innerHeight - r.top + 6, maxHeight: Math.max(140, above) },
    )
  }

  useEffect(() => {
    let stale = false
    void api.worktrees(serverId).then((next) => {
      if (!stale) setTrees(next)
    })
    return () => {
      stale = true
    }
  }, [serverId, dir])

  useEffect(() => {
    if (!open) return
    const away = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false)
    }
    const esc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation()
        setOpen(false)
      }
    }
    // Keep it anchored if the window moves or resizes under it.
    window.addEventListener('resize', place)
    window.addEventListener('scroll', place, true)
    document.addEventListener('mousedown', away)
    document.addEventListener('keydown', esc, true)
    return () => {
      window.removeEventListener('resize', place)
      window.removeEventListener('scroll', place, true)
      document.removeEventListener('mousedown', away)
      document.removeEventListener('keydown', esc, true)
    }
  }, [open])

  if (trees.length < 2) return null

  const current = trees.find((t) => t.isCurrent)

  const choose = async (tree: Worktree) => {
    if (tree.isCurrent) {
      setOpen(false)
      return
    }
    setBusy(true)
    try {
      await api.switchWorktree(serverId, tree.path)
      onSwitched()
    } catch (e) {
      console.error(e)
    } finally {
      setBusy(false)
      setOpen(false)
    }
  }

  return (
    <div className="worktree" ref={root}>
      <button
        ref={trigger}
        className={`branch-btn${open ? ' open' : ''}`}
        onClick={() => {
          if (!open) place()
          setOpen((o) => !o)
        }}
        disabled={busy}
        title={live ? 'Switch worktree — restarts the server there' : 'Switch worktree'}
      >
        <span className="branch-glyph">⎇</span>
        <span className="branch-name">{busy ? 'switching…' : (current?.branch ?? 'detached')}</span>
        <CaretDown weight="bold" />
      </button>

      {open ? (
        <div className="branch-menu" role="listbox" style={box}>
          {live ? <p className="branch-note">Switching restarts the server there.</p> : null}
          {trees.map((tree) => (
            <button
              key={tree.path}
              type="button"
              role="option"
              aria-selected={tree.isCurrent}
              className={`branch-option${tree.isCurrent ? ' on' : ''}`}
              onClick={() => void choose(tree)}
            >
              <span className="branch-option-name">{tree.branch}</span>
              {tree.isMain ? <span className="branch-tag">base</span> : null}
              {tree.isCurrent ? <Check weight="bold" /> : null}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  )
}
