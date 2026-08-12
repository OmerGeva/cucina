import { useEffect, useRef, useState } from 'react'
import type { RefObject } from 'react'

/* The launch screen is the app's own icon, drawn once at size.

   assets/cucina-mark.svg is three things: a ruled field, a horizon, and a sun
   sitting on it. The window's background is already that ruling, so all this
   adds is the line and the disc — and for a beat, before anything else is on
   screen, Cucina opening is the Cucina icon.

   It goes in three movements. The sun comes up slowly behind a line that is
   only as long as it needs to be, and stops where the mark has it. Then it
   climbs and grows while the line runs out to both edges of the window — the
   plate opening into a screen. And when it is up and the line has nowhere left
   to go, the sun leaves for the one place it lives in the running app: the
   hero's disc, whose position is measured rather than guessed, so the two are
   the same circle in the same place and the hand-off is a single frame with
   nothing in it.

   The proportions are the mark's. Its tile is 824 across, its sun r=200 centred
   150 above a horizon 10 thick — so the sun's centre rests three eighths of its
   own diameter above the line, which leaves it a shade under nine tenths risen,
   and the line is a fortieth of the disc. */

/** The sun at rest. */
const SUN = 168
/** Its centre above the horizon: the mark's 150 of 400. */
const CLEAR = SUN * 0.375
/** The horizon. The mark's ratio gives 4, trimmed to 3 for a line this long. */
const RULE = 3
/** How much line there is to begin with. The mark frames its sun in a tile
    twice the width; alone on a window with nothing else on it, less than that
    still reads as a plate and more starts to read as a rule. */
const PLATE = SUN * 1.75
/** Where the horizon sits. Below the middle, so the sun at rest is centred in
    the window rather than riding above it — and so there is somewhere to
    climb to. */
const HORIZON = 0.58
/** Where the sun climbs to, and how much bigger it is when it gets there. */
const TOP = 0.25
const GROW = 1.22
/** Clearance kept under the title bar, whatever the window's height. */
const ROOM = 54

/** The line draws out from the middle, before anything stands on it. */
const DRAW = 400
/** The rise. Slow: this is the part that is the icon. */
const RISE_AT = 200
const RISE = 660
/** A beat at rest, where the screen is exactly the icon. */
const BEAT = 200
/** Up and out: the climb and the line's run to the edges are one movement. */
const ASCEND = 700
/** And the trip to the hero. */
const FLY = 780
/** Waiting on the supervisor is fine; waiting on it forever is not. The sun
    goes at this point whether or not there is anything to land on. */
const PATIENCE = 2600

const OUT = 'cubic-bezier(0.16, 1, 0.3, 1)'
/** From rest to rest — the climb starts and stops, so it eases at both ends. */
const LIFT = 'cubic-bezier(0.5, 0, 0.18, 1)'

interface Props {
  /** The first read has come back, so the hero exists and can be measured. */
  ready: boolean
  /** The hero's disc. Null on any screen that has no hero. */
  disc: RefObject<HTMLElement | null>
  /** The sun has left: the interface has the whole trip to arrive in. */
  onSettle: () => void
  /** It is home. The real disc takes over from this one. */
  onDone: () => void
}

type Phase = 'rising' | 'climbing' | 'leaving'

export default function Intro({ ready, disc, onSettle, onDone }: Props) {
  // One axis each: the arc carries it sideways, the lift carries it up, the
  // sun itself only ever changes size. Kept apart so the flight is two plain
  // curves that happen to cross rather than one path with a corner in it.
  const arc = useRef<HTMLSpanElement>(null)
  const lift = useRef<HTMLSpanElement>(null)
  const sun = useRef<HTMLSpanElement>(null)
  const rule = useRef<HTMLSpanElement>(null)
  const ground = useRef<HTMLSpanElement>(null)

  const [phase, setPhase] = useState<Phase>('rising')
  const [waiting, setWaiting] = useState(true)
  /** How far the climb went, so the flight knows where it is starting from. */
  const climbed = useRef(0)
  const flown = useRef(false)

  // The rise runs the moment the window is up and waits for nothing — covering
  // the first read of the disk and the process table is most of its job.
  useEffect(() => {
    const drawn = rule.current?.animate(
      { transform: ['translateX(-50%) scaleX(0)', 'translateX(-50%) scaleX(1)'] },
      { duration: DRAW, easing: OUT, fill: 'both' },
    )
    const rising = lift.current?.animate(
      { transform: [`translateY(${SUN}px)`, 'translateY(0)'] },
      { duration: RISE, delay: RISE_AT, easing: OUT, fill: 'both' },
    )
    const up = window.setTimeout(() => setPhase('climbing'), RISE_AT + RISE + BEAT)
    const patience = window.setTimeout(() => setWaiting(false), PATIENCE)
    return () => {
      drawn?.cancel()
      rising?.cancel()
      clearTimeout(up)
      clearTimeout(patience)
    }
  }, [])

  useEffect(() => {
    if (phase !== 'climbing') return

    // Both ends are read now rather than written down: the window can be any
    // height, and a climb that ends under the title bar is not a climb.
    const height = window.innerHeight
    const top = Math.max(TOP * height, ROOM + (SUN * GROW) / 2)
    climbed.current = top - (HORIZON * height - RULE / 2 - CLEAR)

    lift.current?.animate(
      { transform: ['translateY(0)', `translateY(${climbed.current}px)`] },
      { duration: ASCEND, easing: LIFT, fill: 'both' },
    )
    sun.current?.animate(
      { transform: ['scale(1)', `scale(${GROW})`] },
      { duration: ASCEND, easing: LIFT, fill: 'both' },
    )
    // The line goes out with it, and lands on both edges as the sun stops.
    rule.current?.animate(
      {
        transform: [
          'translateX(-50%) scaleX(1)',
          `translateX(-50%) scaleX(${window.innerWidth / PLATE})`,
        ],
      },
      { duration: ASCEND, easing: LIFT, fill: 'both' },
    )
    // Flat paper under the horizon is what the unrisen sun was behind. Once it
    // is this far up there is nothing left to hide, and the ruling can carry
    // on down the window as it always does.
    ground.current?.animate(
      { opacity: [1, 0] },
      { duration: 300, delay: ASCEND * 0.45, easing: 'linear', fill: 'both' },
    )

    const done = window.setTimeout(() => setPhase('leaving'), ASCEND)
    return () => clearTimeout(done)
  }, [phase])

  useEffect(() => {
    if (phase !== 'leaving' || flown.current || (!ready && waiting)) return
    const body = sun.current
    if (!body) return
    flown.current = true

    // Measured before the interface is told to arrive, so nothing it does can
    // land between the reading and the flight. Nothing it does moves the hero
    // anyway — that is why the card fades and does not rise.
    const from = body.getBoundingClientRect()
    const to = disc.current?.getBoundingClientRect() ?? null
    onSettle()

    let flight: Animation
    if (to) {
      const dx = to.left + to.width / 2 - (from.left + from.width / 2)
      const dy = to.top + to.height / 2 - (from.top + from.height / 2)
      // Sideways early, down late. Two easings that disagree by that much are
      // what bends the path: the sun runs out across the window and only then
      // drops the last of the way onto the card.
      arc.current?.animate(
        { transform: ['translateX(-50%)', `translateX(-50%) translateX(${dx}px)`] },
        { duration: FLY, easing: 'cubic-bezier(0.36, 0, 0.24, 1)', fill: 'both' },
      )
      lift.current?.animate(
        {
          transform: [
            `translateY(${climbed.current}px)`,
            `translateY(${climbed.current + dy}px)`,
          ],
        },
        { duration: FLY, easing: 'cubic-bezier(0.74, 0, 0.32, 1)', fill: 'both' },
      )
      flight = body.animate(
        { transform: [`scale(${GROW})`, `scale(${to.width / SUN})`] },
        { duration: FLY, easing: 'cubic-bezier(0.5, 0, 0.2, 1)', fill: 'both' },
      )
    } else {
      // No hero to land on — a project with nothing in it, or the harness
      // opened straight onto another screen. It goes out the way it came up.
      flight = body.animate(
        { transform: [`scale(${GROW})`, 'scale(0.9)'], opacity: [1, 1, 0] },
        { duration: FLY, easing: OUT, fill: 'both' },
      )
    }

    rule.current?.animate(
      { opacity: [1, 0] },
      { duration: 420, easing: 'linear', fill: 'both' },
    )

    // Nothing to tear down: the flight is one-way, and the elements it runs on
    // leave with the component the moment it finishes.
    flight.finished.then(onDone, () => {})
  }, [phase, ready, waiting, disc, onSettle, onDone])

  return (
    <div
      className="intro"
      // Nothing behind this is clickable yet, so a press can only mean "get on
      // with it". Cutting to the app is a better answer than ignoring them.
      onPointerDown={onDone}
      style={
        {
          ['--sun']: `${SUN}px`,
          ['--rule']: `${RULE}px`,
          ['--plate']: `${PLATE}px`,
          ['--clear']: `${CLEAR}px`,
          ['--horizon']: `${HORIZON * 100}%`,
        } as React.CSSProperties
      }
    >
      <span className="intro-arc" ref={arc} aria-hidden>
        <span className="intro-lift" ref={lift}>
          <span className="intro-sun" ref={sun} />
        </span>
      </span>
      {/* The ruled ground again, exactly, over the half of the window the sun
          has not risen out of yet. It is invisible for as long as it is
          needed and gone before it is not. */}
      <span className="intro-ground" ref={ground} aria-hidden />
      <span className="intro-rule" ref={rule} aria-hidden />
    </div>
  )
}
