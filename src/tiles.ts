import tile1 from './img/tile-1.jpg'
import tile2 from './img/tile-2.jpg'
import tile3 from './img/tile-3.jpg'
import tile4 from './img/tile-4.jpg'
import tile5 from './img/tile-5.jpg'
import tile6 from './img/tile-6.jpg'

/** Hand-painted maiolica corners. All share the same composition: a plain left
    field for text, the motif in the bottom-right. */
export const TILES = [tile1, tile2, tile3, tile4, tile5, tile6]

/** Stable per-id scatter, so a fresh set of servers already looks varied
    without anyone having to choose. */
function fromId(id: string): number {
  let hash = 0
  for (let i = 0; i < id.length; i++) hash = (hash * 31 + id.charCodeAt(i)) >>> 0
  return hash % TILES.length
}

/** `tile` is 1-based; 0 means "pick one for me". */
export function tileUrl(server: { id: string; tile: number }): string {
  const index =
    server.tile >= 1 && server.tile <= TILES.length ? server.tile - 1 : fromId(server.id)
  return TILES[index]
}

/** A different tile from the current one, so shuffling always visibly changes. */
export function shuffleTile(current: number): number {
  let next = current
  while (next === current) next = 1 + Math.floor(Math.random() * TILES.length)
  return next
}
