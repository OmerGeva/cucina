import { useEffect, useRef, useState } from 'react'
import type { Suggestions, Task } from '../api'
import { api, outcomeOf } from '../api'

interface Props {
  serverId: string
  tasks: Task[]
  /** The task a run is currently using, if any. */
  runningTaskId: string | null
  onRun: (taskId: string) => void
  onRunCommand: (command: string) => void
  onDelete: (taskId: string) => void
  onClose: () => void
}

/** The saved commands on a server, and the ones its project offers. One flat
    list, newest first — no groups, no labels, no pinning. A list read by shape
    stops being scannable the moment it grows furniture. */
export default function Tasks({
  serverId,
  tasks,
  runningTaskId,
  onRun,
  onRunCommand,
  onDelete,
  onClose,
}: Props) {
  const [typed, setTyped] = useState('')
  const [offered, setOffered] = useState<Suggestions | null>(null)
  const [showAll, setShowAll] = useState(false)
  // The full text of whatever long command is under the cursor, and where to
  // put it. Anchored to the menu rather than the row, because the list is a
  // scroll container and anything inside it would be clipped.
  const [peek, setPeek] = useState<{ top: number; text: string } | null>(null)
  const box = useRef<HTMLDivElement>(null)
  const list = useRef<HTMLDivElement>(null)
  const from = useRef<HTMLDivElement>(null)

  // Read when the menu opens, and again whenever the list changes — running a
  // suggestion adds it, and it should leave the offered set at that moment.
  useEffect(() => {
    let stale = false
    void api
      .suggestTasks(serverId)
      .then((found) => {
        if (!stale) setOffered(found)
      })
      .catch(() => setOffered(null))
    return () => {
      stale = true
    }
  }, [serverId, tasks])

  useEffect(() => {
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) onClose()
    }
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    // Capture, so a click on the button that opened this closes it once
    // rather than closing and reopening in the same gesture.
    document.addEventListener('mousedown', away, true)
    window.addEventListener('keydown', esc)
    return () => {
      document.removeEventListener('mousedown', away, true)
      window.removeEventListener('keydown', esc)
    }
  }, [onClose])

  // The suggestions render at the foot of a list that is already scrolled past
  // its own bottom, so opening them would otherwise put them out of sight.
  useEffect(() => {
    if (!showAll) return
    const scroller = list.current
    const header = from.current
    if (!scroller || !header) return
    scroller.scrollTop += header.getBoundingClientRect().top - scroller.getBoundingClientRect().top
  }, [showAll])

  /** Only worth revealing when the row is actually cutting the command off. */
  const onRowEnter = (e: { currentTarget: HTMLElement }, text: string) => {
    const row = e.currentTarget
    const label = row.querySelector('.task-command')
    if (!label || label.scrollWidth <= label.clientWidth) return setPeek(null)
    const anchor = box.current
    if (!anchor) return
    setPeek({ top: row.getBoundingClientRect().top - anchor.getBoundingClientRect().top, text })
  }

  const submit = () => {
    const command = typed.trim()
    if (!command) return
    setTyped('')
    onRunCommand(command)
  }

  const suggestions = offered?.commands ?? []
  // With nothing kept, the suggestions are the menu. With a list of their own
  // they collapse to one line, because the user's set is the point by then.
  const openSuggestions = tasks.length === 0 || showAll
  const source = offered?.source ?? ''

  return (
    <div className="tasks" ref={box}>
      {tasks.length === 0 && suggestions.length === 0 ? (
        <p className="tasks-empty">Nothing here yet. Type a command and it stays on this server.</p>
      ) : null}

      <div className="tasks-list" ref={list} onScroll={() => setPeek(null)}>
        {tasks.map((task) => {
          const running = task.id === runningTaskId
          const outcome = outcomeOf(task, running)
          return (
            <div
              key={task.id}
              role="button"
              tabIndex={0}
              className="task"
              onMouseEnter={(e) => onRowEnter(e, task.command)}
              onMouseLeave={() => setPeek(null)}
              onClick={() => onRun(task.id)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault()
                  onRun(task.id)
                }
              }}
            >
              <span className="task-command">{task.command}</span>
              {outcome ? (
                <span className={`task-outcome${outcome.loud ? ' loud' : ''}`}>
                  {outcome.running ? <span className="task-square" aria-hidden /> : null}
                  {outcome.text}
                </span>
              ) : (
                <span />
              )}
              <button
                className="task-drop"
                aria-label={`Forget ${task.command}`}
                title="Forget this"
                onClick={(e) => {
                  e.stopPropagation()
                  onDelete(task.id)
                }}
              >
                ×
              </button>
            </div>
          )
        })}

        {suggestions.length && openSuggestions ? (
          <>
            <div className="tasks-from" ref={from}>
              <span>from your {source}</span>
              <span className="tasks-hint">tap to run</span>
            </div>
            {suggestions.map((command) => (
              <div
                key={command}
                role="button"
                tabIndex={0}
                className="task offered"
                onMouseEnter={(e) => onRowEnter(e, command)}
                onMouseLeave={() => setPeek(null)}
                onClick={() => onRunCommand(command)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault()
                    onRunCommand(command)
                  }
                }}
              >
                <span className="task-command">{command}</span>
                <span className="task-play" aria-hidden>
                  ▸
                </span>
              </div>
            ))}
          </>
        ) : null}
      </div>

      {suggestions.length && !openSuggestions ? (
        <button className="tasks-more" onClick={() => setShowAll(true)}>
          <span className="caret" aria-hidden>
            ›
          </span>
          {suggestions.length} more from your {source}
        </button>
      ) : null}

      {/* The only way to add one by hand — and it runs at the same time,
          because a command you are not ready to run is not one you would
          have bothered to type. */}
      <div className="tasks-field">
        <span className="prompt" aria-hidden>
          $
        </span>
        <input
          value={typed}
          spellCheck={false}
          autoComplete="off"
          autoCapitalize="off"
          placeholder={tasks.length ? 'run something else' : 'or type your own'}
          aria-label="Run a command"
          onChange={(e) => setTyped(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              submit()
            }
          }}
        />
      </div>

      {peek ? (
        <div className="task-peek" style={{ top: peek.top }}>
          {peek.text}
        </div>
      ) : null}
    </div>
  )
}
