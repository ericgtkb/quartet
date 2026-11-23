/** Shared helpers for the SVG charts in this folder. */

/** About `count` round tick values covering [min, max]. */
export function niceTicks(min: number, max: number, count = 4): number[] {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return [0]
  if (min === max) {
    const pad = Math.abs(min) || 1
    min -= pad / 2
    max += pad / 2
  }
  const rough = (max - min) / count
  const magnitude = 10 ** Math.floor(Math.log10(rough))
  const step = [1, 2, 2.5, 5, 10].map((m) => m * magnitude).find((s) => s >= rough) ?? rough
  const ticks: number[] = []
  for (let t = Math.floor(min / step) * step; t <= max + step * 1e-9; t += step) {
    ticks.push(Number(t.toPrecision(12)))
  }
  if (ticks[ticks.length - 1]! < max)
    ticks.push(Number((ticks[ticks.length - 1]! + step).toPrecision(12)))
  return ticks
}

const compact = new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 })

/** Axis and label formatting: full under 10,000, compact above, short decimals below 1. */
export function formatNumber(v: number): string {
  const a = Math.abs(v)
  if (a >= 10_000) return compact.format(v)
  if (a >= 100 || v === 0) return Math.round(v).toLocaleString('en')
  if (a >= 1) return Number(v.toFixed(2)).toString()
  return Number(v.toPrecision(2)).toString()
}

/** Index of the entry in sorted `xs` nearest to `x`. */
export function nearestIndex(xs: number[], x: number): number {
  let lo = 0
  let hi = xs.length - 1
  while (hi - lo > 1) {
    const mid = (lo + hi) >> 1
    if (xs[mid]! < x) lo = mid
    else hi = mid
  }
  return Math.abs(xs[lo]! - x) <= Math.abs(xs[hi]! - x) ? lo : hi
}

export interface Series {
  label: string
  /** A CSS color, normally `var(--color-series-…)`. */
  color: string
  values: number[]
}
