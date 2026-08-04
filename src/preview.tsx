/* Design harness: renders the real app against a stubbed Tauri bridge, so the
   UI can be reviewed in a plain browser without building the whole bundle.
   Dev-only — `vite build` has a single entry and never picks this up.

   Open http://localhost:1420/preview.html while `npm run app` is running. */

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import type { LogLine, ServerView } from './api'
import './fonts.css'
import './styles.css'

const HOME = '/Users/omergeva'
const since = (mins: number) => Date.now() - mins * 60_000

const server = (
  id: string,
  name: string,
  dir: string,
  command: string,
  group: string,
) => ({ id, name, dir, command, group, tile: 0, env: {}, autoRestart: false, autoStart: false, createdAt: 0 })

const VIEWS: ServerView[] = [
  {
    server: server('demo', 'Demo', `${HOME}/code/demo`, 'python3 -m http.server 8931', ''),
    status: { id: 'demo', state: 'stopped', restarts: 0 },
  },
  {
    server: server(
      'acme-api',
      'Acme API',
      `${HOME}/code/acme/acme-api`,
      'make start',
      'acme',
    ),
    status: {
      id: 'acme-api',
      state: 'running',
      pid: 4711,
      port: 4000,
      startedAt: since(23),
      origin: { kind: 'user' },
      restarts: 0,
    },
  },
  {
    server: server(
      'acme-data-service',
      'Acme Data Service',
      `${HOME}/code/acme/acme-data-service`,
      'poetry run uvicorn app:api --reload',
      'acme',
    ),
    status: {
      id: 'acme-data-service',
      state: 'running',
      pid: 4712,
      port: 8000,
      startedAt: since(23),
      origin: { kind: 'agent', client: 'Claude Code' },
      restarts: 0,
    },
  },
  {
    server: server(
      'acme-fe',
      'Acme FE',
      `${HOME}/code/acme/acme-fe`,
      'make start',
      'acme',
    ),
    status: { id: 'acme-fe', state: 'starting', pid: 4713, startedAt: since(0), restarts: 0 },
  },
  {
    server: server('acme-cms', 'Acme CMS', `${HOME}/code/acme/cms`, 'npm run dev', 'acme'),
    status: { id: 'acme-cms', state: 'crashed', exitCode: 1, restarts: 0 },
  },
]

const LOGS: Record<string, LogLine[]> = {
  'acme-api': [
    'make start — in ~/code/acme/acme-api',
    'go build -o bin/api ./cmd/api',
    'listening on http://127.0.0.1:4000',
    'migrations: 14 applied, 0 pending',
    'GET  /healthz            200   1.2ms',
    'GET  /v1/workspaces      200  18.4ms',
    'POST /v1/sessions        201  42.9ms',
    'GET  /v1/workspaces/9f2  200   7.1ms',
  ].map((text, i) => ({
    seq: i,
    ts: Date.now(),
    stream: i === 0 ? 'system' : 'stdout',
    text,
  })),
  'acme-cms': [
    { seq: 0, ts: Date.now(), stream: 'system', text: 'npm run dev — in ~/code/acme/cms' },
    { seq: 1, ts: Date.now(), stream: 'stderr', text: 'Error: connect ECONNREFUSED 127.0.0.1:5432' },
    { seq: 2, ts: Date.now(), stream: 'stderr', text: '    at TCPConnectWrap.afterConnect [as oncomplete]' },
    { seq: 3, ts: Date.now(), stream: 'system', text: 'Exited with code 1.' },
  ],
}

const RESPONSES: Record<string, unknown> = {
  list_servers: VIEWS,
  list_groups: [{ name: 'acme', icon: '🌿' }],
  list_worktrees: [
    { path: '/x/feat-617-claim-scope-gate', branch: 'feat-617-claim-scope-gate', isMain: true, isCurrent: false },
    { path: '/x/feat-agent-latency-trace', branch: 'feat-agent-latency-trace', isMain: false, isCurrent: false },
    { path: '/x/feat-464-pre-cherrypick', branch: 'feat-464-pre-cherrypick', isMain: false, isCurrent: false },
    { path: '/x/feat-522-amendment-listing-render-bugs', branch: 'feat-522-amendment-listing-render-bugs', isMain: false, isCurrent: false },
    { path: '/x/feat-64-small-number', branch: 'feat-64-small-number', isMain: false, isCurrent: false },
    { path: '/x/feat-533-agent-stream-stall-rescue', branch: 'feat-533-agent-stream-stall-rescue', isMain: false, isCurrent: false },
    { path: '/x/feat-538-checkpoint-doc-surfacing', branch: 'feat-538-checkpoint-doc-surfacing', isMain: false, isCurrent: false },
    { path: '/x/feat-540-oa-examiner-intelligence', branch: 'feat-540-oa-examiner-intelligence', isMain: false, isCurrent: false },
    { path: '/x/feat-542-document-rename', branch: 'feat-542-document-rename', isMain: false, isCurrent: false },
    { path: '/x/feat-550-cross-jurisdiction-foundation', branch: 'feat-550-cross-jurisdiction-foundation', isMain: false, isCurrent: false },
    { path: '/x/feat-580-japan-regulations', branch: 'feat-580-japan-regulations', isMain: false, isCurrent: false },
    { path: '/x/feat-620-diagram-editor', branch: 'feat-620-diagram-editor', isMain: false, isCurrent: true },
  ],
  home_dir: HOME,
  login_item_enabled: false,
  mcp_snippet: JSON.stringify(
    { mcpServers: { cucina: { command: `${HOME}/.local/bin/cucina`, args: ['mcp'] } } },
    null,
    2,
  ),
}

;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
  transformCallback: (callback: unknown) => {
    const id = Math.floor(Math.random() * 1e9)
    ;(window as unknown as Record<string, unknown>)[`_${id}`] = callback
    return id
  },
  invoke: async (command: string, args: Record<string, unknown> = {}) => {
    if (command === 'read_logs') return LOGS[args.id as string] ?? []
    if (command === 'save_server') {
      const next = args.server as ServerView['server']
      const i = VIEWS.findIndex((v) => v.server.id === next.id)
      if (i >= 0) VIEWS[i] = { ...VIEWS[i], server: next }
      console.log(`[harness] save_server ${next.id} → project "${next.group}"`)
      return next
    }
    if (command === 'set_group_icon') {
      console.log(`[harness] set_group_icon ${args.name} → ${args.icon}`)
      return null
    }
    if (command in RESPONSES) return RESPONSES[command]
    // Everything else (start/stop/listen/…) is a no-op in the harness.
    return null
  },
}

const { default: App } = await import('./App')

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
