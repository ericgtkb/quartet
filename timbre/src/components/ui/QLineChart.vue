<script setup lang="ts">
import { computed, ref } from 'vue'
import { formatNumber, nearestIndex, niceTicks, type Series } from './chart'
import { useWidth } from './useWidth'

const props = withDefaults(
  defineProps<{
    title: string
    /** Shared x values, ascending (e.g. beats or agent-steps). */
    x: number[]
    series: Series[]
    xLabel: string
    height?: number
    /** Include zero in the y range (default true). */
    zero?: boolean
    format?: (v: number) => string
  }>(),
  { height: 160, zero: true, format: formatNumber },
)

const root = ref<HTMLElement | null>(null)
const width = useWidth(root)
const margin = { top: 8, right: 56, bottom: 22, left: 44 }
const showTable = ref(false)
const hover = ref<number | null>(null)

const plot = computed(() => ({
  w: Math.max(40, width.value - margin.left - margin.right),
  h: props.height - margin.top - margin.bottom,
}))

const finite = computed(() =>
  props.series.flatMap((s) => s.values).filter((v) => Number.isFinite(v)),
)
const yTicks = computed(() => {
  const vs = finite.value
  if (vs.length === 0) return [0, 1]
  let lo = Math.min(...vs)
  let hi = Math.max(...vs)
  if (props.zero) {
    lo = Math.min(lo, 0)
    hi = Math.max(hi, 0)
    // All zeros: show 0–1 rather than a range around zero.
    if (lo === 0 && hi === 0) hi = 1
  }
  const ticks = niceTicks(lo, hi)
  // Non-negative data never gets negative ticks.
  return lo >= 0 ? ticks.filter((t) => t >= 0) : ticks
})
const yMin = computed(() => yTicks.value[0]!)
const yMax = computed(() => yTicks.value[yTicks.value.length - 1]!)
const xMin = computed(() => props.x[0] ?? 0)
const xMax = computed(() => props.x[props.x.length - 1] ?? 1)

function sx(v: number) {
  const span = xMax.value - xMin.value || 1
  return ((v - xMin.value) / span) * plot.value.w
}
function sy(v: number) {
  const span = yMax.value - yMin.value || 1
  return plot.value.h - ((v - yMin.value) / span) * plot.value.h
}

const paths = computed(() =>
  props.series.map((s) => {
    let d = ''
    let pen = false
    s.values.forEach((v, i) => {
      if (!Number.isFinite(v) || i >= props.x.length) {
        pen = false
        return
      }
      d += `${pen ? 'L' : 'M'}${sx(props.x[i]!).toFixed(1)},${sy(v).toFixed(1)}`
      pen = true
    })
    return d
  }),
)

const xTicks = computed(() => {
  if (props.x.length === 0) return []
  return niceTicks(xMin.value, xMax.value, 3).filter((t) => t >= xMin.value && t <= xMax.value)
})

const last = computed(() => props.x.length - 1)

function onMove(event: PointerEvent) {
  if (props.x.length === 0) return
  const box = (event.currentTarget as SVGElement).getBoundingClientRect()
  const px = event.clientX - box.left - margin.left
  const span = xMax.value - xMin.value || 1
  hover.value = nearestIndex(props.x, xMin.value + (px / plot.value.w) * span)
}

function onKey(event: KeyboardEvent) {
  if (props.x.length === 0) return
  const i = hover.value ?? last.value
  if (event.key === 'ArrowLeft') hover.value = Math.max(0, i - 1)
  else if (event.key === 'ArrowRight') hover.value = Math.min(last.value, i + 1)
  else return
  event.preventDefault()
}

const tooltipLeft = computed(() => {
  if (hover.value === null) return 0
  const x = margin.left + sx(props.x[hover.value]!)
  return x > width.value / 2 ? x - 168 : x + 12
})
</script>

<template>
  <figure ref="root" class="chart">
    <figcaption class="chart__head">
      <span class="chart__title">{{ title }}</span>
      <button type="button" class="chart__toggle" @click="showTable = !showTable">
        {{ showTable ? 'Chart' : 'Table' }}
      </button>
    </figcaption>
    <ul v-if="series.length > 1" class="chart__legend">
      <li v-for="s in series" :key="s.label">
        <span class="chart__key" :style="{ background: s.color }" />{{ s.label }}
      </li>
    </ul>

    <div v-if="showTable" class="chart__table">
      <table>
        <thead>
          <tr>
            <th scope="col">{{ xLabel }}</th>
            <th v-for="s in series" :key="s.label" scope="col">{{ s.label }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(xv, i) in x" :key="i">
            <td>{{ formatNumber(xv) }}</td>
            <td v-for="s in series" :key="s.label">{{ format(s.values[i] ?? NaN) }}</td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-else class="chart__plot">
      <p v-if="x.length === 0" class="chart__empty">No data yet.</p>
      <svg
        v-else
        :width="width"
        :height="height"
        role="img"
        :aria-label="`${title} by ${xLabel}`"
        tabindex="0"
        @pointermove="onMove"
        @pointerleave="hover = null"
        @keydown="onKey"
        @blur="hover = null"
      >
        <g :transform="`translate(${margin.left},${margin.top})`">
          <g class="chart__grid">
            <line v-for="t in yTicks" :key="t" :x1="0" :x2="plot.w" :y1="sy(t)" :y2="sy(t)" />
          </g>
          <g class="chart__axis">
            <text v-for="t in yTicks" :key="t" :x="-6" :y="sy(t)" text-anchor="end" dy="0.32em">
              {{ format(t) }}
            </text>
            <text v-for="t in xTicks" :key="t" :x="sx(t)" :y="plot.h + 16" text-anchor="middle">
              {{ formatNumber(t) }}
            </text>
          </g>
          <path
            v-for="(d, i) in paths"
            :key="series[i]!.label"
            :d="d"
            class="chart__line"
            :style="{ stroke: series[i]!.color }"
          />
          <template v-for="s in series" :key="s.label">
            <circle
              v-if="Number.isFinite(s.values[last])"
              :cx="sx(x[last]!)"
              :cy="sy(s.values[last]!)"
              r="4"
              class="chart__dot"
              :style="{ fill: s.color }"
            />
          </template>
          <text
            v-if="series.length === 1 && Number.isFinite(series[0]!.values[last])"
            class="chart__end"
            :x="sx(x[last]!) + 8"
            :y="sy(series[0]!.values[last]!)"
            dy="0.32em"
          >
            {{ format(series[0]!.values[last]!) }}
          </text>
          <line
            v-if="hover !== null"
            class="chart__crosshair"
            :x1="sx(x[hover]!)"
            :x2="sx(x[hover]!)"
            :y1="0"
            :y2="plot.h"
          />
        </g>
      </svg>
      <div
        v-if="hover !== null"
        class="chart__tooltip"
        role="status"
        :style="{ left: `${tooltipLeft}px` }"
      >
        <div class="chart__tooltip-x">{{ xLabel }} {{ formatNumber(x[hover]!) }}</div>
        <div v-for="s in series" :key="s.label" class="chart__tooltip-row">
          <span class="chart__linekey" :style="{ background: s.color }" />
          <strong>{{ format(s.values[hover] ?? NaN) }}</strong>
          <span>{{ s.label }}</span>
        </div>
      </div>
    </div>
  </figure>
</template>

<style scoped>
.chart {
  margin: 0;
  min-width: 0;
}

.chart__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-2);
  font-size: var(--text-sm);
}

.chart__title {
  font-weight: 600;
}

.chart__toggle {
  padding: 0;
  border: 0;
  background: none;
  color: var(--color-text-muted);
  font: inherit;
  font-size: var(--text-xs);
  cursor: pointer;
  text-decoration: underline;
}

.chart__legend {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1) var(--space-3);
  margin: var(--space-1) 0 0;
  padding: 0;
  list-style: none;
  color: var(--color-text-muted);
  font-size: var(--text-xs);
}

.chart__legend li {
  display: flex;
  align-items: center;
  gap: var(--space-1);
}

.chart__key,
.chart__linekey {
  display: inline-block;
  width: 12px;
  height: 2px;
  border-radius: 1px;
}

.chart__plot {
  position: relative;
  margin-top: var(--space-1);
}

.chart__plot svg {
  display: block;
  overflow: visible;
  touch-action: pan-y;
}

.chart__empty {
  margin: var(--space-4) 0;
  color: var(--color-text-muted);
  font-size: var(--text-sm);
}

.chart__grid line {
  stroke: var(--color-grid);
  stroke-width: 1;
}

.chart__axis text,
.chart__end {
  fill: var(--color-text-muted);
  font-family: var(--font-mono);
  font-size: 10px;
}

.chart__end {
  fill: var(--color-text);
}

.chart__line {
  fill: none;
  stroke-width: 2;
  stroke-linejoin: round;
  stroke-linecap: round;
}

.chart__dot {
  stroke: var(--color-surface);
  stroke-width: 2;
}

.chart__crosshair {
  stroke: var(--color-text-muted);
  stroke-width: 1;
}

.chart__tooltip {
  position: absolute;
  top: 0;
  width: 156px;
  padding: var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-surface);
  box-shadow: var(--shadow-sm);
  font-size: var(--text-xs);
  pointer-events: none;
}

.chart__tooltip-x {
  color: var(--color-text-muted);
}

.chart__tooltip-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.chart__tooltip-row strong {
  font-family: var(--font-mono);
}

.chart__tooltip-row span:last-child {
  color: var(--color-text-muted);
}

.chart__table {
  max-height: 220px;
  overflow: auto;
  font-size: var(--text-xs);
}

.chart__table table {
  width: 100%;
  border-collapse: collapse;
}

.chart__table th,
.chart__table td {
  padding: 2px var(--space-2);
  border-bottom: 1px solid var(--color-grid);
  text-align: right;
  font-family: var(--font-mono);
}

.chart__table th {
  position: sticky;
  top: 0;
  background: var(--color-surface);
  font-family: var(--font-sans);
  font-weight: 600;
}
</style>
