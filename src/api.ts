import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export type RunState = 'stopped' | 'starting' | 'running' | 'crashed'
export type Origin =
  | { kind: 'user' }
  | { kind: 'agent'; client: string; session?: string }
export type StreamKind = 'stdout' | 'stderr' | 'system'

export interface Server {
  id: string
  name: string
  dir: string
  command: string
  group: string
  tile: number
  env: Record<string, string>
  autoRestart: boolean
  autoStart: boolean
  createdAt: number
}

export interface Status {
  id: string
  state: RunState
  pid?: number | null
  port?: number | null
  startedAt?: number | null
  exitCode?: number | null
  origin?: Origin | null
  restarts: number
}

export interface ServerView {
  server: Server
  status: Status
}

export interface Worktree {
  path: string
  branch: string
  isMain: boolean
  isCurrent: boolean
}

export interface UpdateInfo {
  /** null when the installed version is already the newest. */
  version: string | null
  notes: string | null
}

export interface Group {
  name: string
  icon: string
}

export interface LogLine {
  seq: number
  ts: number
  stream: StreamKind
  text: string
}

/** A command kept on a server — a migration, a seed, a test run. Distinct from
    the server's own `command`, which is the one that starts it. */
export interface Task {
  id: string
  command: string
  /** How the last run ended: 0 succeeded, n failed, and null alongside a
      `lastRunAt` means a signal ended it — what pressing Stop looks like. */
  lastExit?: number | null
  /** Null until it has run once. */
  lastRunAt?: number | null
}

export interface Run {
  runId: string
  serverId: string
  taskId: string
  command: string
  startedAt: number
  /** Null while it is still going. The only "is it live" test there is. */
  endedAt?: number | null
  exitCode?: number | null
  origin?: Origin | null
  lastOutputAt: number
}

/** A process holding a port that Cucina does not own. Observed, never stored —
    every scan replaces the list outright. */
export interface Stray {
  port: number
  pid: number
  /** The command line as the kernel has it: resolved shims, absolute
      interpreter paths, whatever flags an agent added. Not a start command. */
  command: string
  /** Null when it could not be read — usually a directory since deleted. */
  dir?: string | null
  /** Seconds since it started. */
  age: number
  /** The terminal behind it, `ZSH S004`. Null means nothing is behind it. */
  owner?: string | null
}

export interface Suggestions {
  /** The manifest the commands were read from, for the menu header. */
  source: string
  commands: string[]
}

export type CucinaEvent =
  | ({ type: 'status' } & Status)
  | { type: 'log'; id: string; lines: LogLine[] }
  | { type: 'serversChanged' }
  | { type: 'show' }
  | ({ type: 'run' } & Run)
  | { type: 'tasks'; id: string; tasks: Task[] }

export const blankServer = (): Server => ({
  id: '',
  name: '',
  dir: '',
  command: '',
  group: '',
  tile: 0,
  env: {},
  autoRestart: false,
  autoStart: false,
  createdAt: 0,
})

export const isLive = (view: ServerView) =>
  view.status.state === 'running' || view.status.state === 'starting'

export interface Section {
  /** Empty name means the loose servers that belong to no project. */
  name: string
  views: ServerView[]
}

/** Loose servers first, then each project in the order it first appears. */
export function sections(views: ServerView[]): Section[] {
  const loose = views.filter((v) => !v.server.group)
  const names: string[] = []
  for (const v of views) {
    if (v.server.group && !names.includes(v.server.group)) names.push(v.server.group)
  }
  const grouped = names.map((name) => ({
    name,
    views: views.filter((v) => v.server.group === name),
  }))
  return loose.length ? [{ name: '', views: loose }, ...grouped] : grouped
}

/** The folder a directory sits in — a sensible default project name. */
export function parentFolder(dir: string): string {
  const parts = dir.split('/').filter(Boolean)
  return parts.length >= 2 ? parts[parts.length - 2] : ''
}

export const api = {
  list: () => invoke<ServerView[]>('list_servers'),
  start: (id: string) => invoke<void>('start_server', { id }),
  stop: (id: string) => invoke<void>('stop_server', { id }),
  restart: (id: string) => invoke<void>('restart_server', { id }),
  save: (server: Server) => invoke<Server>('save_server', { server }),
  remove: (id: string) => invoke<void>('delete_server', { id }),
  groups: () => invoke<Group[]>('list_groups'),
  worktrees: (id: string) => invoke<Worktree[]>('list_worktrees', { id }),
  switchWorktree: (id: string, path: string) =>
    invoke<void>('switch_worktree', { id, path }),
  logs: (id: string, tail = 500) => invoke<LogLine[]>('read_logs', { id, tail }),
  clearLogs: (id: string) => invoke<void>('clear_logs', { id }),
  tasks: (id: string) => invoke<Task[]>('list_tasks', { id }),
  runTask: (id: string, taskId: string) => invoke<Run>('run_task', { id, taskId }),
  runCommand: (id: string, command: string) => invoke<Run>('run_command', { id, command }),
  stopRun: (runId: string) => invoke<void>('stop_run', { runId }),
  deleteTask: (id: string, taskId: string) => invoke<void>('delete_task', { id, taskId }),
  readRun: (id: string) => invoke<Run | null>('read_run', { id }),
  suggestTasks: (id: string) => invoke<Suggestions>('suggest_tasks', { id }),
  strays: () => invoke<Stray[]>('scan_strays'),
  stopStray: (pid: number) => invoke<void>('stop_stray', { pid }),
  openUrl: (url: string) => invoke<void>('open_url', { url }),
  reveal: (path: string) => invoke<void>('reveal_in_finder', { path }),
  pickDirectory: () => invoke<string | null>('pick_directory'),
  loginItem: () => invoke<boolean>('login_item_enabled'),
  setLoginItem: (enabled: boolean) => invoke<void>('set_login_item', { enabled }),
  installCli: () => invoke<string>('install_cli'),
  mcpSnippet: () => invoke<string>('mcp_snippet'),
  homeDir: () => invoke<string>('home_dir'),
  version: () => invoke<string>('app_version'),
  checkUpdate: () => invoke<UpdateInfo>('check_update'),
  installUpdate: () => invoke<void>('install_update'),
}

export const onEvent = (handler: (event: CucinaEvent) => void) =>
  listen<CucinaEvent>('cucina://event', (e) => handler(e.payload))

// ---- formatting -----------------------------------------------------------

export function shortenPath(path: string, home: string): string {
  if (home && path.startsWith(home)) return '~' + path.slice(home.length)
  return path
}

export function uptime(since: number, now: number): string {
  const secs = Math.max(0, Math.floor((now - since) / 1000))
  if (secs < 60) return `${secs}s`
  if (secs < 3600) return `${Math.floor(secs / 60)}m`
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return m ? `${h}h ${m}m` : `${h}h`
}

/** Deliberately not a type predicate: a finished run is still a `Run`, so
    narrowing on this would tell the compiler the wrong thing. */
export const isRunning = (run?: Run | null): boolean => Boolean(run && run.endedAt == null)

/** The elapsed clock on a live run: `0:04`, `12:31`, `1:04:02`. */
export function elapsed(since: number, now: number): string {
  const secs = Math.max(0, Math.floor((now - since) / 1000))
  const pad = (n: number) => String(n).padStart(2, '0')
  const m = Math.floor(secs / 60)
  if (m < 60) return `${m}:${pad(secs % 60)}`
  return `${Math.floor(m / 60)}:${pad(m % 60)}:${pad(secs % 60)}`
}

export interface Outcome {
  text: string
  /** Vermilion. A failure, or a run in progress. */
  loud: boolean
  /** Draws the filled square, which in this app only ever means running. */
  running: boolean
}

/** What a task's last run came to. Null before it has ever run, which reads as
    "no outcome to report" rather than as a success. */
export function outcomeOf(task: Task, running: boolean): Outcome | null {
  if (running) return { text: 'running', loud: true, running: true }
  if (task.lastRunAt == null) return null
  // A signal leaves no exit code, and the only thing that sends one here is
  // the user pressing Stop — so it is reported as their doing, not a failure.
  if (task.lastExit == null) return { text: 'stopped', loud: false, running: false }
  return { text: `exit ${task.lastExit}`, loud: task.lastExit !== 0, running: false }
}

const SPELLED = [
  'no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven',
  'eight', 'nine', 'ten', 'eleven', 'twelve',
]

/** Small counts read better spelled out in the hero caption. */
export const spell = (n: number) => (n < SPELLED.length ? SPELLED[n] : String(n))

/** Nothing is behind it. The reason strays exist as a page at all. */
export const isOrphan = (stray: Stray): boolean => !stray.owner

/** How long a stray has been loose: `22m`, `4h 12m`, `5d 8h`. Coarser than
    `uptime` on purpose — at this age the minutes stopped mattering. */
export function ageOf(secs: number): string {
  if (secs < 60) return `${Math.max(0, Math.floor(secs))}s`
  const m = Math.floor(secs / 60)
  if (m < 60) return `${m}m`
  const h = Math.floor(m / 60)
  if (h < 24) return h ? `${h}h ${m % 60}m` : `${m % 60}m`
  const d = Math.floor(h / 24)
  return `${d}d ${h % 24}h`
}

/** "scanned 40s ago" — the freshness of the list, in the same place in every
    state, because a stale list of processes is worse than no list. */
export function sinceScan(at: number, now: number): string {
  const secs = Math.max(0, Math.floor((now - at) / 1000))
  if (secs < 5) return 'just now'
  return `${ageOf(secs)} ago`
}

/** The agents we can recognise on sight. Anything else is still credited, it
    just appears under whatever name it gave us, without a mark. */
export type AgentBrand = 'claude-code' | 'codex' | 'cursor'

export interface Agent {
  brand: AgentBrand | null
  label: string
  /** What the agent called the conversation it was in. Nothing in MCP carries
      this, so it is only here when the agent bothered to pass it. */
  session: string
}

const BRANDS: [AgentBrand, RegExp, string][] = [
  ['claude-code', /claude/, 'Claude Code'],
  ['codex', /codex|openai/, 'Codex'],
  ['cursor', /cursor/, 'Cursor'],
]

/** MCP clients name themselves in the initialize handshake, but the exact
    string drifts between releases — so match loosely, and when nothing matches
    keep the name rather than dropping the attribution back to "an agent". */
export function agentOf(origin?: Origin | null): Agent | null {
  if (!origin || origin.kind !== 'agent') return null
  const raw = origin.client.trim()
  const session = (origin.session ?? '').trim()
  const hit = BRANDS.find(([, pattern]) => pattern.test(raw.toLowerCase()))
  if (hit) return { brand: hit[0], label: hit[2], session }
  const label = raw && raw.toLowerCase() !== 'agent' ? raw : 'an agent'
  return { brand: null, label, session }
}
