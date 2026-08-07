import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export type RunState = 'stopped' | 'starting' | 'running' | 'crashed'
export type Origin = { kind: 'user' } | { kind: 'agent'; client: string }
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

export type CucinaEvent =
  | ({ type: 'status' } & Status)
  | { type: 'log'; id: string; lines: LogLine[] }
  | { type: 'serversChanged' }
  | { type: 'show' }

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
  openUrl: (url: string) => invoke<void>('open_url', { url }),
  reveal: (path: string) => invoke<void>('reveal_in_finder', { path }),
  pickDirectory: () => invoke<string | null>('pick_directory'),
  loginItem: () => invoke<boolean>('login_item_enabled'),
  setLoginItem: (enabled: boolean) => invoke<void>('set_login_item', { enabled }),
  installCli: () => invoke<string>('install_cli'),
  mcpSnippet: () => invoke<string>('mcp_snippet'),
  homeDir: () => invoke<string>('home_dir'),
  version: () => invoke<string>('app_version'),
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

const SPELLED = [
  'no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven',
  'eight', 'nine', 'ten', 'eleven', 'twelve',
]

/** Small counts read better spelled out in the hero caption. */
export const spell = (n: number) => (n < SPELLED.length ? SPELLED[n] : String(n))

export function originLabel(origin?: Origin | null): string | null {
  if (!origin || origin.kind !== 'agent') return null
  return origin.client && origin.client !== 'agent' ? origin.client : 'an agent'
}
