import type { Frame } from '@/api/types'

/** A small frame; `overrides` replaces top-level fields. */
export function makeFrame(beat: number, x: number[], overrides: Partial<Frame> = {}): Frame {
  return {
    beat,
    width: 100,
    height: 100,
    quartetlings: {
      id: x.map((_, i) => i + 1),
      voice: x.map((_, i) => (i % 4) as 0 | 1 | 2 | 3),
      x,
      y: x.map(() => 50),
      heading: x.map(() => 0),
      energy: x.map(() => 1),
      breath: x.map(() => 1),
      singing: x.map(() => 0),
      inChord: x.map(() => 0),
      age: x.map(() => beat),
      generation: x.map(() => 0),
    },
    motes: { x: [10, 20], y: [10, 20] },
    statics: { x: [50], y: [50], radius: [10] },
    stats: { motesEaten: 0, deaths: 0, chordMembers: 0, births: 0 },
    ...overrides,
  }
}
