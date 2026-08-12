/* Design harness: renders the real app against a stubbed Tauri bridge, so the
   UI can be reviewed in a plain browser without building the whole bundle.
   Dev-only — `vite build` has a single entry and never picks this up.

   Open http://localhost:1420/preview.html while `npm run app` is running. */

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import type { LogLine, Run, ServerView, Stray, Task } from './api'
import './fonts.css'
import './styles.css'

const HOME = '/Users/you'
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
    server: server('docs', 'Docs', `${HOME}/code/docs`, 'python3 -m http.server 8931', ''),
    status: { id: 'docs', state: 'stopped', restarts: 0 },
  },
  {
    server: server(
      'api',
      'API',
      `${HOME}/code/acme/api`,
      'make start',
      'acme',
    ),
    status: {
      id: 'api',
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
      'data-service',
      'Data Service',
      `${HOME}/code/acme/data-service`,
      'poetry run uvicorn app:api --reload',
      'acme',
    ),
    status: {
      id: 'data-service',
      state: 'running',
      pid: 4712,
      port: 8000,
      startedAt: since(23),
      // The raw strings clients send in the MCP handshake, not display names —
      // the point of the harness is to prove the matching works.
      origin: {
        kind: 'agent',
        client: 'claude-code',
        session: 'Chemical patents analysis tool',
      },
      restarts: 0,
    },
  },
  {
    server: server('search', 'Search', `${HOME}/code/acme/search`, 'cargo watch -x run', 'acme'),
    status: {
      id: 'search',
      state: 'running',
      pid: 4714,
      port: 7700,
      startedAt: since(8),
      origin: { kind: 'agent', client: 'codex', session: 'Redline export without color' },
      restarts: 0,
    },
  },
  {
    server: server('worker', 'Worker', `${HOME}/code/acme/worker`, 'npm run worker', 'acme'),
    status: {
      id: 'worker',
      state: 'running',
      pid: 4715,
      port: 9200,
      startedAt: since(2),
      origin: { kind: 'agent', client: 'cursor-vscode' },
      restarts: 0,
    },
  },
  {
    server: server(
      'web',
      'Web',
      `${HOME}/code/acme/web`,
      'make start',
      'acme',
    ),
    status: { id: 'web', state: 'starting', pid: 4713, startedAt: since(0), restarts: 0 },
  },
  {
    server: server('cms', 'CMS', `${HOME}/code/acme/cms`, 'npm run dev', 'acme'),
    status: { id: 'cms', state: 'crashed', exitCode: 1, restarts: 0 },
  },
]

const LOGS: Record<string, LogLine[]> = {
  'api': [
    'make start — in ~/code/acme/api',
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
  // The agent-started server needs output of its own: it is the one that
  // shows the attribution chip, so it is the one worth screenshotting.
  'data-service': [
    'poetry run uvicorn app:api --reload — in ~/code/acme/data-service',
    'INFO:     Will watch for changes in these directories: ["/code/acme/data-service"]',
    'INFO:     Uvicorn running on http://127.0.0.1:8000 (Press CTRL+C to quit)',
    'INFO:     Started reloader process [88214] using WatchFiles',
    'INFO:     Application startup complete.',
    'INFO:     127.0.0.1:53318 - "GET /v1/documents?limit=25 HTTP/1.1" 200 OK',
    'INFO:     127.0.0.1:53319 - "POST /v1/embeddings HTTP/1.1" 201 Created',
    'INFO:     127.0.0.1:53320 - "GET /v1/documents/4c1a HTTP/1.1" 200 OK',
  ].map((text, i) => ({
    seq: i,
    ts: Date.now(),
    stream: i === 0 ? 'system' : ('stdout' as const),
    text,
  })),
  'cms': [
    { seq: 0, ts: Date.now(), stream: 'system', text: 'npm run dev — in ~/code/acme/cms' },
    { seq: 1, ts: Date.now(), stream: 'stderr', text: 'Error: connect ECONNREFUSED 127.0.0.1:5432' },
    { seq: 2, ts: Date.now(), stream: 'stderr', text: '    at TCPConnectWrap.afterConnect [as oncomplete]' },
    { seq: 3, ts: Date.now(), stream: 'system', text: 'Exited with code 1.' },
  ],
}

const task = (command: string, lastExit: number | null, ranMinsAgo?: number): Task => ({
  id: command.replace(/[^a-z0-9]+/gi, '-').toLowerCase(),
  command,
  lastExit,
  lastRunAt: ranMinsAgo === undefined ? null : since(ranMinsAgo),
})

/* Twelve entries with three long ones — the list scrolls and ellipsises, which
   is the state worth looking at. `docs` is left with none, so the first-open
   case is one click away at ?at=server:docs. */
const TASKS: Record<string, Task[]> = {
  'data-service': [
    task('bin/rails db:migrate', 0, 4),
    task('bin/rails db:seed', 0, 40),
    task('npx prisma migrate deploy --schema=./prisma/schema.prisma', 1, 55),
    task('bin/rails console', null, 61),
    task('bundle exec rspec spec/models/dataset_spec.rb', 0, 90),
    task('npx tsc --noEmit --project ./tsconfig.build.json', 2, 120),
    task('tail -f log/development.log', null, 200),
    task('bin/rails db:rollback STEP=1', 0, 240),
    task('make lint', 0, 300),
    task('python manage.py createsuperuser', 0, 360),
    task('bundle exec rake assets:precompile', 0, 420),
    task('alembic upgrade head', 0, 480),
  ],
  'api': [task('make migrate', 0, 12), task('make seed', 1, 30)],
  'search': [],
}

/* Whatever `?run=` asks for, so each output-box state can be opened directly:
   ?at=server:data-service&run=running | done | failed | quiet */
const RUNS: Record<string, Run> = {
  running: {
    runId: 'run-1',
    serverId: 'data-service',
    taskId: 'bin-rails-db-migrate',
    command: 'bin/rails db:migrate',
    startedAt: Date.now() - 4_000,
    lastOutputAt: Date.now() - 500,
  },
  done: {
    runId: 'run-2',
    serverId: 'data-service',
    taskId: 'bin-rails-db-migrate',
    command: 'bin/rails db:migrate',
    startedAt: Date.now() - 8_000,
    endedAt: Date.now() - 4_600,
    exitCode: 0,
    lastOutputAt: Date.now() - 4_600,
  },
  failed: {
    runId: 'run-3',
    serverId: 'data-service',
    taskId: 'npx-prisma-migrate-deploy',
    command: 'npx prisma migrate deploy --schema=./prisma/schema.prisma',
    startedAt: Date.now() - 9_000,
    endedAt: Date.now() - 7_800,
    exitCode: 1,
    lastOutputAt: Date.now() - 7_800,
  },
  // Four minutes of silence, still going — the case a special "stuck" state
  // would have got wrong.
  quiet: {
    runId: 'run-4',
    serverId: 'data-service',
    taskId: 'bin-rails-console',
    command: 'bin/rails console',
    startedAt: Date.now() - 252_000,
    lastOutputAt: Date.now() - 248_000,
  },
}

const RUN_OUTPUT: Record<string, string[]> = {
  running: [
    '$ bin/rails db:migrate',
    '== 20260721142233 AddIndexToDatasets: migrating ==========',
    '-- add_index(:datasets, [:workspace_id, :created_at])',
    '   -> 0.0431s',
  ],
  done: [
    '$ bin/rails db:migrate',
    '== 20260721142310 BackfillDatasetCounts: migrated (2.9102s) ==',
    'exited 0 after 3.4s',
  ],
  failed: [
    '$ npx prisma migrate deploy --schema=./prisma/schema.prisma',
    'Error: P3009 migrate found failed migrations in the target database',
    'exited 1 after 1.2s',
  ],
  quiet: ['$ bin/rails console', 'Loading development environment (Rails 7.1.3)', 'irb(main):001:0>'],
}

/* Six results, covering every row shape at once: orphan and terminal-owned,
   a long line that has to ellipsise, and one with no working directory whose
   command promotes into the first line. `?strays=` picks the state:
   ?at=strays&strays=none | one | slow | fail */
const STRAYS: Stray[] = [
  {
    port: 5173,
    pid: 80338,
    command: 'node /Users/you/code/acme/web/node_modules/.bin/vite --host',
    dir: `${HOME}/code/acme/web`,
    age: 156_000,
  },
  {
    port: 3000,
    pid: 44219,
    command: 'next-server (v14.2.3)',
    dir: `${HOME}/code/acme/marketing`,
    age: 15_120,
    owner: 'ZSH S004',
  },
  {
    port: 8000,
    pid: 12904,
    command:
      '/Users/you/.venvs/spike/bin/python -m uvicorn app.main:app --reload --port 8000 --log-level debug --workers 1 --host 0.0.0.0',
    dir: `${HOME}/code/scratch/api-spike`,
    age: 183_600,
  },
  {
    port: 4321,
    pid: 60117,
    command: 'node /Users/you/code/personal/notes-site/node_modules/astro/astro.js dev',
    dir: `${HOME}/code/personal/notes-site`,
    age: 21_780,
  },
  {
    port: 7777,
    pid: 51204,
    command: 'bun run --hot src/index.ts',
    dir: `${HOME}/code/acme/edge`,
    age: 1_320,
    owner: 'ZSH S011',
  },
  {
    port: 54123,
    pid: 77650,
    command: 'node /Users/you/.npm/_npx/8ab3f1/node_modules/.bin/http-server -p 54123 --cors -c-1',
    dir: null,
    age: 460_800,
  },
]

const strayState = new URLSearchParams(location.search).get('strays') ?? ''
const STRAY_RESULT: Record<string, Stray[]> = {
  none: [],
  one: STRAYS.slice(0, 1),
}

const RESPONSES: Record<string, unknown> = {
  list_servers: VIEWS,
  list_groups: [{ name: 'acme', icon: '' }],
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

const wanted = new URLSearchParams(location.search).get('run') ?? ''
const staged = RUNS[wanted]

// A run's output belongs to the server's own stream, so the harness appends it
// the same way the supervisor does — that is the whole point of the merge.
if (staged) {
  const log = LOGS[staged.serverId] ?? []
  LOGS[staged.serverId] = log.concat(
    (RUN_OUTPUT[wanted] ?? []).map((text, i) => ({
      seq: log.length + i,
      ts: staged.lastOutputAt,
      stream: text.startsWith('$ ') || text.startsWith('exited')
        ? ('system' as const)
        : text.startsWith('Error:')
          ? ('stderr' as const)
          : ('stdout' as const),
      text,
    })),
  )
}

;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
  transformCallback: (callback: unknown) => {
    const id = Math.floor(Math.random() * 1e9)
    ;(window as unknown as Record<string, unknown>)[`_${id}`] = callback
    return id
  },
  invoke: async (command: string, args: Record<string, unknown> = {}) => {
    if (command === 'scan_strays') {
      // `slow` holds the scan open long enough to look at it; `fail` throws the
      // way the real probe does, so the failed state keeps its last timestamp.
      if (strayState === 'slow') await new Promise((go) => setTimeout(go, 30_000))
      if (strayState === 'fail') throw 'lsof: exited 1 — no such file or directory'
      return STRAY_RESULT[strayState] ?? STRAYS
    }
    if (command === 'stop_stray') {
      // `stopping` holds the kill open. The busy row is the one state on this
      // page that is normally over before anyone can look at it.
      if (strayState === 'stopping') await new Promise((go) => setTimeout(go, 30_000))
      const i = STRAYS.findIndex((s) => s.pid === args.pid)
      if (i >= 0) STRAYS.splice(i, 1)
      console.log(`[harness] stop_stray ${args.pid}`)
      return null
    }
    if (command === 'read_logs') return LOGS[args.id as string] ?? []
    if (command === 'list_tasks') return TASKS[args.id as string] ?? []
    if (command === 'suggest_tasks') {
      // A directory with no package.json, Gemfile or Makefile offers nothing,
      // which with no tasks kept is the only way to see the empty menu.
      if (args.id === 'docs') return { source: '', commands: [] }
      // Mirrors `manifest::suggest`: whatever the project offers, minus what
      // the user already keeps. Offering a command twice is the bug this
      // filter exists to prevent, so the harness has to have it too.
      const kept = new Set((TASKS[args.id as string] ?? []).map((t) => t.command))
      const commands = [
        'bin/rails db:migrate',
        'bin/rails db:seed',
        'bin/rails console',
        'bin/rails db:rollback STEP=1',
        'bundle exec rspec',
        'bundle exec rubocop --autocorrect-all --config ./.rubocop.yml',
      ].filter((c) => !kept.has(c))
      return { source: commands.length ? 'Gemfile' : '', commands }
    }
    if (command === 'read_run') {
      return staged && staged.serverId === args.id ? staged : null
    }
    if (command === 'save_server') {
      const next = args.server as ServerView['server']
      const i = VIEWS.findIndex((v) => v.server.id === next.id)
      if (i >= 0) VIEWS[i] = { ...VIEWS[i], server: next }
      console.log(`[harness] save_server ${next.id} → project "${next.group}"`)
      return next
    }
    if (command in RESPONSES) return RESPONSES[command]
    // Everything else (start/stop/listen/…) is a no-op in the harness.
    return null
  },
}

// preview.html?at=server:api | at=settings | at=project:acme
const at = new URLSearchParams(location.search).get('at')
if (at) {
  const [kind, arg] = at.split(':')
  if (kind === 'add') (window as unknown as Record<string, unknown>).__cucinaAdd = true
  ;(window as unknown as Record<string, unknown>).__cucinaRoute =
    kind === 'server' ? { kind: 'server', id: arg }
    : kind === 'project' ? { kind: 'project', name: arg }
    : { kind }
}

// preview.html?slow=6 — every timer and every animation at a sixth of speed,
// so a launch that is over in two and a half seconds can be looked at properly.
// Timers are stretched here; the animations themselves are caught as they
// appear, which covers the ones CSS starts as well as the ones code does.
const slow = Number(new URLSearchParams(location.search).get('slow')) || 1
if (slow > 1) {
  const timer = window.setTimeout.bind(window)
  window.setTimeout = ((fn: TimerHandler, ms = 0, ...rest: unknown[]) =>
    timer(fn, ms * slow, ...(rest as []))) as typeof window.setTimeout
  setInterval(() => {
    for (const a of document.getAnimations()) a.playbackRate = 1 / slow
  }, 25)
}

const { default: App } = await import('./App')

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
