import { useCallback, useEffect, useRef, useState } from 'react'
import { CaretLeft } from '@phosphor-icons/react'
import type { LogLine, Run, ServerView, Task, Worktree } from '../api'
import { agentOf, api, elapsed, isRunning, onEvent, shortenPath, uptime } from '../api'
import AgentMark from './AgentMark'
import { Dot, stateWord } from './bits'
import Tasks from './Tasks'
import Worktrees from './Worktrees'

interface Props {
  view: ServerView
  lines: LogLine[]
  home: string
  /** Read once in App and passed down. */
  trees: Worktree[]
  now: number
  onBack: () => void
  onStart: () => void
  onStop: () => void
  onRestart: () => void
  onEdit: () => void
  onOpenPort: (port: number) => void
  onReveal: () => void
  onClearLogs: () => void
  onSwitched: () => void
}

export default function Detail({
  view,
  lines,
  home,
  trees,
  now,
  onBack,
  onStart,
  onStop,
  onRestart,
  onEdit,
  onOpenPort,
  onReveal,
  onClearLogs,
  onSwitched,
}: Props) {
  const { server, status } = view
  const live = status.state === 'running' || status.state === 'starting'
  const agent = agentOf(status.origin)

  const [tasks, setTasks] = useState<Task[]>([])
  const [run, setRun] = useState<Run | null>(null)
  const [menu, setMenu] = useState(false)

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

  useEffect(() => {
    let stale = false
    setMenu(false)
    void api.tasks(server.id).then((list) => !stale && setTasks(list))
    void api.readRun(server.id).then((found) => !stale && setRun(found))
    return () => {
      stale = true
    }
  }, [server.id])

  useEffect(() => {
    const pending = onEvent((event) => {
      if (event.type === 'tasks' && event.id === server.id) {
        setTasks(event.tasks)
      } else if (event.type === 'run' && event.serverId === server.id) {
        const { type: _type, ...next } = event
        setRun(next as Run)
      }
    })
    return () => {
      void pending.then((unlisten) => unlisten())
    }
  }, [server.id])

  // A clock, but only while a run is actually going — the elapsed figure and
  // the quiet notice both need it, and neither exists when nothing is running.
  const [tick, setTick] = useState(() => Date.now())
  const runLive = isRunning(run)
  useEffect(() => {
    if (!runLive) return
    setTick(Date.now())
    const id = setInterval(() => setTick(Date.now()), 1000)
    return () => clearInterval(id)
  }, [runLive, run?.runId])

  const onScroll = () => {
    const el = scroll.current
    if (el) stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40
  }

  const runTask = useCallback(
    (taskId: string) => {
      setMenu(false)
      void api.runTask(server.id, taskId).catch(console.error)
    },
    [server.id],
  )

  const runCommand = useCallback(
    (command: string) => {
      setMenu(false)
      void api.runCommand(server.id, command).catch(console.error)
    },
    [server.id],
  )

  return (
    <>
      <header className="detail-head">
        <button className="back" onClick={onBack}>
          <CaretLeft weight="bold" />
          All servers
        </button>

        <div className="detail-title">
          <h1>{server.name}</h1>

          {/* A filled square and the word — the state needs no capsule. */}
          <span className="chip">
            <Dot state={status.state} />
            {stateWord(status.state)}
          </span>
          {/* Who, and what they were doing — one chip, because the session
              name means nothing without the agent it belongs to. */}
          {agent ? (
            <span className="chip agent" title={`Started by ${agent.label}`}>
              {agent.brand ? <AgentMark brand={agent.brand} /> : null}
              {/* One inline run, so the two typefaces share a baseline of
                  their own accord and the chip centres them as a block. */}
              <span className="agent-text">
                by {agent.label}
                {agent.session ? <span className="session">{agent.session}</span> : null}
              </span>
            </span>
          ) : null}

          {status.port ? (
            <button
              className="detail-port"
              onClick={() => onOpenPort(status.port!)}
              title={`Open localhost:${status.port}`}
            >
              :{status.port}
            </button>
          ) : null}
        </div>

        <div className="detail-meta">
          <button className="path-btn" onClick={onReveal} title="Reveal in Finder">
            {shortenPath(server.dir, home)}
          </button>
          <Worktrees serverId={server.id} trees={trees} live={live} onSwitched={onSwitched} />
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
          {/* Beside Stop and Restart because it is the same kind of thing:
              something you do to this server, from this screen. */}
          <div className="tasks-anchor">
            <button
              className={`btn${menu ? ' on' : ''}`}
              aria-expanded={menu}
              onClick={() => setMenu((open) => !open)}
            >
              Tasks
              <span className="caret" aria-hidden>
                {menu ? '⌃' : '⌄'}
              </span>
            </button>
            {menu ? (
              <Tasks
                serverId={server.id}
                tasks={tasks}
                runningTaskId={runLive && run ? run.taskId : null}
                onRun={runTask}
                onRunCommand={runCommand}
                onDelete={(taskId) => void api.deleteTask(server.id, taskId).catch(console.error)}
                onClose={() => setMenu(false)}
              />
            ) : null}
          </div>
          <span className="spacer" />
          <button className="btn quiet small" onClick={onEdit}>
            Edit
          </button>
        </div>
      </header>

      {/* One window. A task's output goes into the server's own stream, in the
          order it happened, delimited by the `$ command` and `exited n` lines
          the supervisor writes around it. While a run is going the header says
          which command is talking and offers the one control it needs. */}
      <div className="well">
        {runLive ? <span className="run-rule" aria-hidden /> : null}

        <div className="well-head">
          <span className="well-title">Output</span>
          {runLive && run ? (
            <>
              <span className="run-square" aria-hidden />
              <span className="run-command" title={run.command}>
                {run.command}
              </span>
              <span className="run-outcome">{elapsed(run.startedAt, tick)}</span>
            </>
          ) : (
            <>
              <span className="well-source">the server</span>
              {lines.length ? (
                <span className="well-count">
                  {lines.length} line{lines.length === 1 ? '' : 's'}
                </span>
              ) : null}
            </>
          )}
          <span className="spacer" />

          {runLive && run ? (
            <button
              className="btn small"
              onClick={() => void api.stopRun(run.runId).catch(console.error)}
            >
              Stop
            </button>
          ) : null}
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
