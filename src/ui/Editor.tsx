import { useState } from 'react'
import type { CSSProperties } from 'react'
import type { Server } from '../api'
import { api, parentFolder, shortenPath } from '../api'
import Select from './Select'
import { pitchFor, shufflePitch } from '../rings'

interface Props {
  server: Server
  home: string
  groups: string[]
  /** The command line a stray was observed running, when this sheet was opened
      by adopting one. Set only in that case — see the note by the field. */
  observed?: string
  onSaved: (server: Server) => void
  onDeleted: (id: string) => void
  onCancel: () => void
}

type Pair = { key: string; value: string }

export default function Editor({
  server,
  home,
  groups,
  observed,
  onSaved,
  onDeleted,
  onCancel,
}: Props) {
  const [name, setName] = useState(server.name)
  const [dir, setDir] = useState(server.dir)
  const [command, setCommand] = useState(server.command)
  const [group, setGroup] = useState(server.group)
  const [tile, setTile] = useState(server.tile)
  /** True while typing a project name that doesn't exist yet. */
  const [naming, setNaming] = useState(
    Boolean(server.group) && !groups.includes(server.group),
  )
  const [autoRestart, setAutoRestart] = useState(server.autoRestart)
  const [autoStart, setAutoStart] = useState(server.autoStart)
  const [pairs, setPairs] = useState<Pair[]>(
    Object.entries(server.env).map(([key, value]) => ({ key, value })),
  )
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const isNew = server.id === ''

  const choose = async () => {
    const picked = await api.pickDirectory()
    if (!picked) return
    setDir(picked)
    // Sensible defaults from the path, both still editable.
    if (!name.trim()) setName(picked.split('/').filter(Boolean).pop() ?? '')
    if (!group.trim()) {
      const suggested = parentFolder(picked)
      if (suggested) {
        setGroup(suggested)
        setNaming(!groups.includes(suggested))
      }
    }
  }

  const save = async () => {
    setBusy(true)
    setError(null)
    const env: Record<string, string> = {}
    for (const { key, value } of pairs) {
      const k = key.trim()
      if (k) env[k] = value
    }
    try {
      onSaved(
        await api.save({
          ...server,
          name: name.trim(),
          dir: dir.trim(),
          command: command.trim(),
          group: group.trim(),
          tile,
          env,
          autoRestart,
          autoStart,
        }),
      )
    } catch (e) {
      setError(String(e))
      setBusy(false)
    }
  }

  const remove = async () => {
    setBusy(true)
    try {
      await api.remove(server.id)
      onDeleted(server.id)
    } catch (e) {
      setError(String(e))
      setBusy(false)
    }
  }

  const setPair = (i: number, patch: Partial<Pair>) =>
    setPairs((prev) => prev.map((p, j) => (i === j ? { ...p, ...patch } : p)))

  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && onCancel()}>
      <div
        className="sheet"
        onKeyDown={(e) => {
          if (e.key === 'Escape') onCancel()
          if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) void save()
        }}
      >
        <div className="sheet-head">
          <h2>{isNew ? 'Add a server' : server.name}</h2>
          <span className="rule" />
        </div>

        <div className="sheet-body">
          {error ? <div className="error">{error}</div> : null}

          <div className="field-pair">
            <div className="field">
              <label htmlFor="cu-name">Name</label>
              <input
                id="cu-name"
                type="text"
                value={name}
                autoFocus
                placeholder="api"
                onChange={(e) => setName(e.target.value)}
              />
            </div>

            <div className="field">
              <label htmlFor="cu-group">Project</label>
              <Select
                id="cu-group"
                value={naming ? ' new' : group}
                onChange={(next) => {
                  if (next === ' new') {
                    setNaming(true)
                    setGroup('')
                  } else {
                    setNaming(false)
                    setGroup(next)
                  }
                }}
                options={[
                  { value: '', label: 'No project' },
                  ...groups.map((g) => ({ value: g, label: g })),
                  { value: ' new', label: 'New project…' },
                ]}
              />
            </div>
          </div>

          {naming ? (
            <div className="field">
              <label htmlFor="cu-new-group">New project name</label>
              <input
                id="cu-new-group"
                type="text"
                value={group}
                autoFocus
                placeholder="acme"
                onChange={(e) => setGroup(e.target.value)}
              />
            </div>
          ) : null}

          <div className="field">
            <label htmlFor="cu-dir">Directory</label>
            <div className="field-row">
              <input
                id="cu-dir"
                type="text"
                value={shortenPath(dir, home)}
                placeholder="~/code/api"
                onChange={(e) => setDir(e.target.value)}
              />
              <button className="btn" onClick={choose}>
                Choose…
              </button>
            </div>
          </div>

          {/* A scanned command line is a runtime artefact, not a start command:
              absolute interpreter paths, resolved bin shims, flags an agent
              added. Adopting one unedited would fill Cucina with servers that
              fail on their second launch — so the field is flagged, and the
              line as observed stays visible underneath to correct against. */}
          <div className={`field${observed ? ' flagged' : ''}`}>
            <label htmlFor="cu-cmd">Command</label>
            <input
              id="cu-cmd"
              type="text"
              value={command}
              placeholder="npm run dev"
              onChange={(e) => setCommand(e.target.value)}
            />
            {observed ? (
              <>
                <span className="hint loud">
                  This is what the process was running, not a command you would type. Shorten it to
                  the one that starts this server.
                </span>
                <pre className="code observed">{observed}</pre>
              </>
            ) : (
              <span className="hint">Runs in your login shell.</span>
            )}
          </div>

          <div className="field">
            <div className="field-head">
              <label>Environment</label>
              <button
                className="link-btn"
                onClick={() => setPairs((prev) => [...prev, { key: '', value: '' }])}
              >
                Add a variable
              </button>
            </div>
            {pairs.length ? (
              <div className="env-list">
                {pairs.map((pair, i) => (
                  <div className="env-row" key={i}>
                    <input
                      value={pair.key}
                      placeholder="KEY"
                      onChange={(e) => setPair(i, { key: e.target.value })}
                    />
                    <input
                      value={pair.value}
                      placeholder="value"
                      onChange={(e) => setPair(i, { value: e.target.value })}
                    />
                    <button
                      className="icon-btn"
                      title="Remove"
                      onClick={() => setPairs((prev) => prev.filter((_, j) => j !== i))}
                    >
                      ×
                    </button>
                  </div>
                ))}
              </div>
            ) : (
              <span className="hint">None set.</span>
            )}
          </div>

          <div className="bottom-row">
            <div className="toggles">
            <label className="toggle">
              <input
                type="checkbox"
                checked={autoRestart}
                onChange={(e) => setAutoRestart(e.target.checked)}
              />
              <span>
                Restart if it crashes
                <small>Gives up after five failures in a minute.</small>
              </span>
            </label>

            <label className="toggle">
              <input
                type="checkbox"
                checked={autoStart}
                onChange={(e) => setAutoStart(e.target.checked)}
              />
              <span>
                Start when Cucina opens
                <small>For the one you always want running.</small>
              </span>
            </label>
            </div>

            <div className="tile-pick">
              <button
                className="tile-preview"
                title="Shuffle the ring signature"
                onClick={() => setTile((t) => shufflePitch(t))}
                style={
                  {
                    ['--pitch' as string]: pitchFor({ id: server.id || name || 'new', tile }),
                  } as CSSProperties
                }
              />
              <button className="link-btn centred" onClick={() => setTile((t) => shufflePitch(t))}>
                Shuffle
              </button>
            </div>
          </div>
        </div>

        <div className="sheet-foot">
          {observed ? (
            <span className="sheet-note">
              The process holding the port keeps running. This is still a stray until you stop it
              and start the server from here.
            </span>
          ) : null}
          {!isNew ? (
            <button className="btn danger" onClick={remove} disabled={busy}>
              Delete
            </button>
          ) : null}
          <span className="spacer" />
          <button className="btn quiet" onClick={onCancel}>
            Cancel
          </button>
          <button className="btn primary" onClick={save} disabled={busy}>
            {isNew ? 'Add' : 'Save'}
          </button>
        </div>
      </div>
    </div>
  )
}
