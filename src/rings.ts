/** The ring signature a running card carries in its bottom-right corner.
 *
 *  Only the pitch varies — the spacing between the concentric hairlines — and
 *  it is derived from the server id, so a fresh set of servers already looks
 *  varied without anyone having to choose. This replaces the maiolica tile
 *  scatter, and reuses the same `tile` column on the server record to store an
 *  explicit choice, so there is no migration. */
export const PITCHES = [6, 7, 8, 9, 10, 11]

/** Stable per-id scatter. Same hash the tiles used. */
function fromId(id: string): number {
  let hash = 0
  for (let i = 0; i < id.length; i++) hash = (hash * 31 + id.charCodeAt(i)) >>> 0
  return hash % PITCHES.length
}

/** `tile` is 1-based; 0 means "pick one for me". Returns a CSS length. */
export function pitchFor(server: { id: string; tile: number }): string {
  const index =
    server.tile >= 1 && server.tile <= PITCHES.length ? server.tile - 1 : fromId(server.id)
  return `${PITCHES[index]}px`
}

/** A different pitch from the current one, so shuffling always visibly changes. */
export function shufflePitch(current: number): number {
  let next = current
  while (next === current) next = 1 + Math.floor(Math.random() * PITCHES.length)
  return next
}
