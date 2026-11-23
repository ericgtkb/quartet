import type { Frame } from '@/api/types'

export interface Pose {
  x: number
  y: number
  heading: number
}

/** Shortest signed difference b − a between two angles, in (−π, π]. */
export function angleDelta(a: number, b: number): number {
  let d = (b - a) % (2 * Math.PI)
  if (d > Math.PI) d -= 2 * Math.PI
  if (d <= -Math.PI) d += 2 * Math.PI
  return d
}

/**
 * The pose of `slot` at fraction `alpha` (0..1) of the way from `prev` to
 * `curr`. A slot whose quartetling changed (respawn) snaps to `curr`.
 */
export function poseAt(prev: Frame | null, curr: Frame, slot: number, alpha: number): Pose {
  const c = curr.quartetlings
  const x = c.x[slot] ?? 0
  const y = c.y[slot] ?? 0
  const heading = c.heading[slot] ?? 0
  const p = prev?.quartetlings
  if (!p || p.id[slot] !== c.id[slot]) return { x, y, heading }
  const t = Math.min(Math.max(alpha, 0), 1)
  const px = p.x[slot] ?? x
  const py = p.y[slot] ?? y
  const ph = p.heading[slot] ?? heading
  return {
    x: px + (x - px) * t,
    y: py + (y - py) * t,
    heading: ph + angleDelta(ph, heading) * t,
  }
}

/** Interpolation progress for a frame that arrived `sinceMs` ago. */
export function alphaFor(sinceMs: number, beatMs: number): number {
  return beatMs > 0 ? Math.min(Math.max(sinceMs / beatMs, 0), 1) : 1
}
