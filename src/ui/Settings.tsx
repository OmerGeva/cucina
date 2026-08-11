import { useEffect, useState } from 'react'
import type { UpdateInfo } from '../api'
import { api } from '../api'

interface Props {
  onScanStrays: () => void
}

/** Everything configurable lives here, so there is one place to look. */
export default function Settings({ onScanStrays }: Props) {
  const [atLogin, setAtLogin] = useState(false)
  const [snippet, setSnippet] = useState('')
  const [notice, setNotice] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [version, setVersion] = useState('')
  const [update, setUpdate] = useState<UpdateInfo | null>(null)
  const [checking, setChecking] = useState(false)
  const [installing, setInstalling] = useState(false)

  useEffect(() => {
    void api.loginItem().then(setAtLogin)
    void api.mcpSnippet().then(setSnippet)
    void api.version().then(setVersion)
  }, [])

  const toggleLogin = async (next: boolean) => {
    setAtLogin(next)
    setError(null)
    try {
      await api.setLoginItem(next)
    } catch (e) {
      setAtLogin(!next)
      setError(String(e))
    }
  }

  // Deliberately manual: nothing reaches the network unless this is pressed.
  const checkUpdate = async () => {
    setChecking(true)
    setError(null)
    setUpdate(null)
    try {
      setUpdate(await api.checkUpdate())
    } catch (e) {
      setError(String(e))
    } finally {
      setChecking(false)
    }
  }

  const applyUpdate = async () => {
    setInstalling(true)
    setError(null)
    try {
      // Restarts on success, so nothing after this runs.
      await api.installUpdate()
    } catch (e) {
      setError(String(e))
      setInstalling(false)
    }
  }

  const install = async () => {
    setError(null)
    try {
      setNotice(await api.installCli())
    } catch (e) {
      setError(String(e))
    }
  }

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(snippet)
      setCopied(true)
      setTimeout(() => setCopied(false), 1600)
    } catch {
      setError('Could not reach the clipboard — select the text and copy it.')
    }
  }

  return (
    <div className="page-scroll">
      <div className="page-head">
        <h1>Settings</h1>
      </div>

      {error ? <div className="error" style={{ marginBottom: 18 }}>{error}</div> : null}

      <section className="panel">
        <div className="panel-row">
          <div>
            <h2 className="panel-title">Cucina</h2>
            <p className="prose">Keep your local servers on the heat.</p>
          </div>
          {version ? <span className="version">v{version}</span> : null}
        </div>

        <div className="panel-row">
          <div>
            <strong className="panel-label">Updates</strong>
            <p className="prose">
              {update?.version
                ? `Version ${update.version} is available.`
                : update
                  ? 'Cucina is up to date.'
                  : 'Checks GitHub for a newer release. Nothing about you is sent.'}
            </p>
          </div>
          {update?.version ? (
            <button className="btn primary" onClick={applyUpdate} disabled={installing}>
              {installing ? 'Installing…' : 'Install and restart'}
            </button>
          ) : (
            <button className="btn" onClick={checkUpdate} disabled={checking}>
              {checking ? 'Checking…' : 'Check for updates'}
            </button>
          )}
        </div>
        {update?.version && update.notes ? <pre className="code">{update.notes}</pre> : null}
      </section>

      <section className="panel">
        <h2 className="panel-title">Startup</h2>
        <label className="toggle">
          <input
            type="checkbox"
            checked={atLogin}
            onChange={(e) => void toggleLogin(e.target.checked)}
          />
          <span>
            Open Cucina at login
            <small>
              Sits in the menu bar so the command line and coding agents can always reach it.
            </small>
          </span>
        </label>
      </section>

      {/* The rail entry is absent at zero, which is most days. This line is the
          only reason the page stays reachable when there is nothing on it. */}
      <section className="panel">
        <h2 className="panel-title">Strays</h2>
        <div className="panel-row">
          <div>
            <strong className="panel-label">Scan for strays</strong>
            <p className="prose">
              Processes holding a port that Cucina does not own — a dev server left behind by a
              shell that went away. Cucina looks each time the window comes forward, and puts them
              in the rail when it finds any.
            </p>
          </div>
          <button className="btn" onClick={onScanStrays}>
            Open
          </button>
        </div>
      </section>

      <section className="panel">
        <h2 className="panel-title">Agents</h2>
        <p className="prose">
          Coding agents can hand a server to Cucina and walk away — no background shell to hold
          open, nothing to remember to kill. Whatever they start shows up in your menu bar, and you
          can stop it yourself.
        </p>

        <div className="panel-row">
          <div>
            <strong className="panel-label">Command line</strong>
            <p className="prose">
              Installs <code>cucina</code> into <code>~/.local/bin</code>.
            </p>
          </div>
          <button className="btn" onClick={install}>
            Install
          </button>
        </div>
        {notice ? <div className="notice">{notice}</div> : null}

        <pre className="code">
{`cucina                    what's running
cucina up api --wait      start it, block until it's listening
cucina up acme         start every server in a project
cucina logs api --tail 50 see why it broke
cucina down api           stop it`}
        </pre>

        <div className="panel-row">
          <div>
            <strong className="panel-label">MCP server</strong>
            <p className="prose">
              Native tools for list, start, stop, restart and logs. Any id also accepts a project
              name, so an agent can bring a whole stack up in one call.
            </p>
          </div>
          <button className="btn" onClick={copy}>
            {copied ? 'Copied' : 'Copy config'}
          </button>
        </div>
        <pre className="code">{snippet || '…'}</pre>
      </section>
    </div>
  )
}
