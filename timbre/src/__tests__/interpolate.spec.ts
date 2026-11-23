import { describe, expect, it } from 'vitest'
import { alphaFor, angleDelta, poseAt } from '@/stage/interpolate'
import { makeFrame } from './fixtures'

describe('interpolation', () => {
  it('lerps position between beats', () => {
    const prev = makeFrame(1, [10])
    const curr = makeFrame(2, [20])
    expect(poseAt(prev, curr, 0, 0.5).x).toBeCloseTo(15)
    expect(poseAt(prev, curr, 0, 2).x).toBeCloseTo(20) // clamped
  })

  it('snaps a respawned slot instead of sliding across the arena', () => {
    const prev = makeFrame(1, [10])
    const curr = makeFrame(2, [90])
    curr.quartetlings.id[0] = 99
    expect(poseAt(prev, curr, 0, 0.5).x).toBe(90)
  })

  it('turns the short way around', () => {
    expect(angleDelta(3, -3)).toBeCloseTo(2 * Math.PI - 6)
    const prev = makeFrame(1, [10])
    const curr = makeFrame(2, [10])
    prev.quartetlings.heading[0] = 3
    curr.quartetlings.heading[0] = -3
    const h = poseAt(prev, curr, 0, 0.5).heading
    expect(Math.abs(h)).toBeGreaterThan(3) // passes through ±π, not 0
  })

  it('computes alpha from elapsed time', () => {
    expect(alphaFor(50, 100)).toBe(0.5)
    expect(alphaFor(500, 100)).toBe(1)
    expect(alphaFor(10, 0)).toBe(1)
  })
})
