// Draws the menu bar marks from geometry.
//
// The app icon is NOT generated here — it comes from assets/icon-source.png
// via `npm run icons:app` (which shells out to `tauri icon`). This script only
// writes the two monochrome template images macOS needs for the menu bar,
// because those must be pure black + alpha and readable at 18pt.
//
//   src-tauri/icons/tray-idle.png     nothing running  (outlined farfalle)
//   src-tauri/icons/tray-active.png   something running (filled farfalle)

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

// ---- the farfalle ---------------------------------------------------------

/**
 * A bow of pasta on the 36-unit grid: pinched at the waist, widening to two
 * ruffled ends. `scale` shrinks the whole form, which is how the outlined
 * variant is produced (full shape minus a smaller copy of itself).
 */
function farfalle(scale = 1) {
  const CX = 18
  const CY = 18
  const W = 15.4 * scale // half-width
  const WAIST = 2.15 * scale // half-height at the pinch
  const WING = 9.4 * scale // half-height at the ends
  const RUFFLE = 1.15 * scale // depth of the edge scallops

  return (x, y) => {
    const dx = x - CX
    const dy = y - CY

    // Ruffled outer edge: the ends scallop up and down.
    const edge = W - RUFFLE * (0.5 - 0.5 * Math.cos((dy / WING) * Math.PI * 2.6))
    if (Math.abs(dx) > edge) return false

    // Waist-to-wing profile. The exponent keeps the pinch tight and lets the
    // wings flare late, which is what reads as "bow" rather than "triangle".
    const t = Math.min(1, Math.abs(dx) / W)
    const half = WAIST + (WING - WAIST) * Math.pow(t, 0.62)
    return Math.abs(dy) <= half
  }
}

function trayMark(filled) {
  const S = 36 // 18pt at 2x
  const outer = farfalle(1)
  if (filled) return encodePNG(S, S, render(S, outer))

  // Outline: the shape with a slightly smaller copy knocked out of it.
  const inner = farfalle(0.76)
  return encodePNG(S, S, render(S, (x, y) => outer(x, y) && !inner(x, y)))
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
