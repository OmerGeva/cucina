import { useEffect, useRef, useState } from 'react'
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
  const root = useRef<HTMLDivElement>(null)

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
    document.addEventListener('mousedown', away)
    document.addEventListener('keydown', esc, true)
    return () => {
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
        className={`branch-btn${open ? ' open' : ''}`}
        onClick={() => setOpen((o) => !o)}
        disabled={busy}
        title={live ? 'Switch worktree — restarts the server there' : 'Switch worktree'}
      >
        <span className="branch-glyph">⎇</span>
        <span className="branch-name">{busy ? 'switching…' : (current?.branch ?? 'detached')}</span>
        <CaretDown weight="bold" />
      </button>

      {open ? (
        <div className="branch-menu" role="listbox">
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
