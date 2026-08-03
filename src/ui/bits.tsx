import type { RunState } from '../api'

export const Dot = ({ state }: { state: RunState }) => (
  <span className={`dot ${state}`} aria-hidden />
)

const WORDS: Record<RunState, string> = {
  running: 'running',
  starting: 'starting',
  crashed: 'crashed',
  stopped: 'stopped',
}

export const stateWord = (state: RunState) => WORDS[state]

export const Chip = ({ state }: { state: RunState }) => (
  <span className={`chip ${state}`}>
    <Dot state={state} />
    {WORDS[state]}
  </span>
)
