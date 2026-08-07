// Draws the menu bar marks from geometry.
//
// The app icon is NOT generated here — it comes from assets/icon-source.png
// via `npm run icons:app` (which shells out to `tauri icon`). This script only
// writes the two monochrome template images macOS needs for the menu bar,
// because those must be pure black + alpha and readable at 18pt.
//
//   src-tauri/icons/tray-idle.png     nothing running  (outlined disc)
//   src-tauri/icons/tray-active.png   something running (filled disc)

import zlib from 'node:zlib'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = path.dirname(fileURLToPath(import.meta.url))
const OUT = path.join(HERE, '..', 'src-tauri', 'icons')

// ---- PNG encoding ---------------------------------------------------------

const CRC_TABLE = (() => {
  const t = new Int32Array(256)
  for (let n = 0; n < 256; n++) {
    let c = n
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
    t[n] = c
  }
  return t
})()

function crc32(buf) {
  let c = -1
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8)
  return (c ^ -1) >>> 0
}

function chunk(type, data) {
  const len = Buffer.alloc(4)
  len.writeUInt32BE(data.length)
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data])
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(crc32(body))
  return Buffer.concat([len, body, crc])
}

function encodePNG(w, h, rgba) {
  const stride = w * 4
  const raw = Buffer.alloc((stride + 1) * h)
  for (let y = 0; y < h; y++) {
    raw[y * (stride + 1)] = 0 // filter: none
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride)
  }
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(w, 0)
  ihdr.writeUInt32BE(h, 4)
  ihdr[8] = 8 // bit depth
  ihdr[9] = 6 // truecolour + alpha
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ])
}

// ---- supersampled rasteriser ---------------------------------------------

const SS = 8

function render(size, inside) {
  const w = size * SS
  const cov = new Float32Array(size * size)
  for (let py = 0; py < w; py++) {
    const y = (py + 0.5) / SS
    for (let px = 0; px < w; px++) {
      const x = (px + 0.5) / SS
      if (inside(x, y)) cov[((py / SS) | 0) * size + ((px / SS) | 0)] += 1
    }
  }
  const out = Buffer.alloc(size * size * 4)
  const per = SS * SS
  for (let i = 0; i < size * size; i++) {
    out[i * 4 + 3] = Math.round((cov[i] / per) * 255) // black + alpha
  }
  return out
}

// ---- the sun --------------------------------------------------------------

/**
 * A disc sitting on a rule. `scale` shrinks the disc, which is how the outlined
 * variant is produced (full circle minus a smaller copy of itself).
 */
function disc(scale = 1) {
  const CX = 18
  const CY = 17
  const R = 9.5 * scale
  return (x, y) => {
    const dx = x - CX
    const dy = y - CY
    return dx * dx + dy * dy <= R * R
  }
}

/** The horizon. Full-width, so the mark keeps its footprint when idle. */
function horizon(x, y) {
  return x >= 4 && x <= 32 && y >= 26 && y <= 28.5
}

function trayMark(filled) {
  const S = 36 // 18pt at 2x
  const outer = disc(1)
  if (filled) return encodePNG(S, S, render(S, (x, y) => outer(x, y) || horizon(x, y)))

  // Idle: the disc as a ring, the rule still solid.
  const inner = disc(0.68)
  return encodePNG(
    S,
    S,
    render(S, (x, y) => (outer(x, y) && !inner(x, y)) || horizon(x, y)),
  )
}

fs.mkdirSync(OUT, { recursive: true })
const wrote = []
for (const [name, buf] of [
  ['tray-idle.png', trayMark(false)],
  ['tray-active.png', trayMark(true)],
]) {
  fs.writeFileSync(path.join(OUT, name), buf)
  wrote.push(`${name} (${buf.length}B)`)
}
console.log('cucina menu bar marks →', wrote.join(', '))
