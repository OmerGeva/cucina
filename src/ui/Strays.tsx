import { useEffect, useRef, useState } from 'react'
import type { Stray } from '../api'
import { ageOf, isOrphan, shortenPath, sinceScan, spell } from '../api'

/** What Cucina runs to find these. Printed under the skeleton rows, because a
    program reading your process table should say so. */
const PROBE = 'lsof -nP -iTCP -sTCP:LISTEN'

/** How long the receipt for a stopped process stays up. */
const RECEIPT_MS = 8000
/** How long Stop stays armed before it disarms itself. */
const ARMED_MS = 3000

interface Props {
  strays: Stray[]
  /** When the last good scan finished, in epoch ms. Null before the first. */
  at: number | null
  scanning: boolean
  /** The raw failure from the probe, kept verbatim. */
  error: string | null
  home: string
  onScan: () => void
  onStop: (pid: number) => Promise<void>
  onAdopt: (stray: Stray) => void
}

interface Receipt {
  port: number
  pid: number
}

export default function Strays({
  strays,
  at,
  scanning,
  error,
  home,
  onScan,
  onStop,
  onAdopt,
}: Props) {
  // The row whose Stop is armed. One at a time: arming a second would leave two
  // vermilion buttons on screen and no way to tell which one you meant.
  const [armed, setArmed] = useState<number | null>(null)
  const [going, setGoing] = useState<number | null>(null)
  const [receipt, setReceipt] = useState<Receipt | null>(null)
  // Long command lines ellipsise; a click opens the one you want to read.
  const [opened, setOpened] = useState<number | null>(null)
  const [now, setNow] = useState(() => Date.now())
  const timers = useRef<number[]>([])

  useEffect(() => {
    const kept = timers.current
    return () => kept.forEach(clearTimeout)
  }, [])

  // The timestamp has to keep reading true while the page sits open, and this
  // is the only clock on the screen — so it ticks slowly and only while there
  // is something for it to say.
  useEffect(() => {
    if (at == null) return
    const id = setInterval(() => setNow(Date.now()), 5000)
    return () => clearInterval(id)
  }, [at])

  useEffect(() => {
    if (armed == null) return
    const id = window.setTimeout(() => setArmed(null), ARMED_MS)
    timers.current.push(id)
    return () => clearTimeout(id)
  }, [armed])

  const stop = async (stray: Stray) => {
    setArmed(null)
    setGoing(stray.pid)
    try {
      await onStop(stray.pid)
      setReceipt({ port: stray.port, pid: stray.pid })
      const id = window.setTimeout(() => setReceipt(null), RECEIPT_MS)
      timers.current.push(id)
    } finally {
      setGoing(null)
    }
  }

  const orphans = strays.filter(isOrphan).length
  const stamp = at == null ? '' : `scanned ${sinceScan(at, now)}`

  return (
    <div className="page-scroll">
      <div className="page-head">
        <h1>Strays</h1>
        <span className="spacer" />
        {/* Same place in every state. A failed scan keeps the last good
            timestamp rather than pretending the list is fresh. */}
        <span className={`scan-stamp${scanning ? ' loud' : ''}`}>
          {scanning ? 'Scanning' : error && at != null ? `last good scan ${sinceScan(at, now)}` : stamp}
        </span>
        <button
          className={`btn${error ? ' primary' : ''}`}
          onClick={onScan}
          disabled={scanning}
        >
          {error ? 'Try again' : 'Scan'}
        </button>
      </div>

      {!scanning && !error && strays.length ? (
        <p className="page-note">{caption(strays.length, orphans)}</p>
      ) : null}

      {error ? (
        <div className="scan-failed">
          <strong>
            <span className="scan-square" aria-hidden />
            The scan did not finish
          </strong>
          <p>
            Cucina could not read the list of listening processes, so it does not know what is out
            there. Nothing has changed and nothing was stopped.
          </p>
          <pre className="code">{error}</pre>
        </div>
      ) : scanning ? (
        <div className="panel strays">
          <span className="scan-rule" aria-hidden />
          <div className="stray skeleton">
            <span />
            <span className="bar port" />
            <span className="bar wide" />
          </div>
          <div className="stray skeleton">
            <span />
            <span className="bar port" />
            <span className="bar mid" />
          </div>
          <div className="stray skeleton">
            <span />
            <span className="bar port" />
            <span className="bar narrow" />
          </div>
          <p className="scan-probe">{PROBE}</p>
        </div>
      ) : strays.length === 0 ? (
        <div className="panel strays">
          {receipt ? <Receipt receipt={receipt} /> : null}
          <div className="strays-empty">
            <span className="empty-square" aria-hidden />
            <strong>Nothing loose</strong>
            <p>
              Every process holding a port right now is one of yours.
              <br />
              Cucina checks each time the window comes forward.
            </p>
          </div>
        </div>
      ) : (
        <div className="panel strays">
          {receipt ? <Receipt receipt={receipt} /> : null}

          <div className="stray head">
            <span />
            <span>Port</span>
            <span>Where</span>
            <span className="right">Age</span>
            <span />
          </div>

          {strays.map((stray) => {
            const where = stray.dir ? shortenPath(stray.dir, home) : stray.command
            const under = stray.dir ? stray.command : 'No working directory'
            return (
              <div
                key={`${stray.pid}:${stray.port}`}
                className={`stray${opened === stray.pid ? ' open' : ''}`}
                onClick={() => setOpened((was) => (was === stray.pid ? null : stray.pid))}
              >
                {/* Only an orphan is marked. A process with a terminal behind
                    it is somebody's live work and should look like it. */}
                {isOrphan(stray) ? (
                  <span className="stray-square" title="Nothing is behind this one — no terminal, no shell" />
                ) : (
                  <span />
                )}

                <span className="stray-port">{stray.port}</span>

                <span className="stray-where">
                  <span className="stray-dir">
                    <span className="stray-path">{where}</span>
                    {stray.owner ? <span className="stray-owner">{stray.owner}</span> : null}
                  </span>
                  <span className={`stray-command${stray.dir ? '' : ' caption'}`}>{under}</span>
                </span>

                <span className="stray-age">
                  {ageOf(stray.age)}
                  <span className="stray-pid">{stray.pid}</span>
                </span>

                <span className="stray-acts" onClick={(e) => e.stopPropagation()}>
                  {armed === stray.pid ? (
                    <>
                      {/* Arms in place rather than opening a sheet: this is one
                          row among six, and a dialog each turns clearing up
                          into six dialogs. There is no undo on offer because
                          there is no undo — the receipt takes its place. */}
                      <button className="btn danger small" onClick={() => void stop(stray)}>
                        Confirm
                      </button>
                      <button className="btn quiet small" onClick={() => setArmed(null)}>
                        Cancel
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        className="btn small"
                        disabled={going === stray.pid}
                        onClick={() => setArmed(stray.pid)}
                      >
                        {going === stray.pid ? 'Stopping…' : 'Stop'}
                      </button>
                      <button
                        className="btn small"
                        disabled={!stray.dir}
                        title={
                          stray.dir
                            ? 'Keep this as a server'
                            : 'Nothing to run it in — there is no working directory'
                        }
                        onClick={() => onAdopt(stray)}
                      >
                        Adopt
                      </button>
                    </>
                  )}
                </span>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

/** Names the port and the dead pid. Not an undo — you cannot un-kill a process,
    and restarting it would be a different one with a different pid. */
function Receipt({ receipt }: { receipt: Receipt }) {
  return (
    <div className="stray-receipt">
      <span className="receipt-square" aria-hidden />
      :{receipt.port} stopped — pid {receipt.pid} is gone
    </div>
  )
}

const cap = (word: string) => word.charAt(0).toUpperCase() + word.slice(1)

/** How many there are, and — on its own line — what the mark on a row means.
    A red square that appears on some rows and not others is a legend the
    reader was never given, and a legend buried at the end of a count is one
    they will not find. The rows without it name their terminal instead, which
    is the other half of the same sentence. */
function caption(total: number, orphans: number) {
  const head =
    total === 1
      ? 'One process is holding a port and it is not one of yours.'
      : `${cap(spell(total))} processes are holding a port and none of them are Cucina's.`

  const legend =
    orphans === 0 ? (
      total === 1 ? (
        'A terminal is still behind it.'
      ) : (
        'Every one still has a terminal behind it.'
      )
    ) : (
      <>
        <span className="stray-square inline" aria-hidden />
        {orphans === total
          ? total === 1
            ? ' Nothing is behind it'
            : ' Nothing is behind any of them'
          : ` ${cap(spell(orphans))} of them have nothing behind them`}
        {' — no terminal, no shell.'}
      </>
    )

  return (
    <>
      {head}
      <span className="page-legend">{legend}</span>
    </>
  )
}
