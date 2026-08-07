import type { RunState } from '../api'

/** A filled square, not a dot — the only round things in this design are the
    ring discs and the rail's live pip. */
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

