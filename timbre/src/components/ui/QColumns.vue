<script setup lang="ts">
import { computed, ref } from 'vue'
import { formatNumber } from './chart'

/**
 * A compact single-series column chart (e.g. one voice's histogram). Columns
 * are capped at 24px, rounded at the data end, separated by a 2px gap; each
 * is its own hover/focus target. Small multiples share `yMax`.
 */
const props = withDefaults(
  defineProps<{
    label: string
    counts: number[]
    /** Category label per column, for tooltips (e.g. a bin's range). */
    names: string[]
    color: string
    yMax?: number
    height?: number
  }>(),
  { yMax: 0, height: 48 },
)

const active = ref<number | null>(null)
const max = computed(() => Math.max(props.yMax, ...props.counts, 1))
const slot = computed(() => 100 / Math.max(1, props.counts.length))

function bar(i: number) {
  const h = (props.counts[i]! / max.value) * props.height
  return { h, y: props.height - h }
}
</script>

<template>
  <div class="columns" role="group" :aria-label="label">
    <svg
      class="columns__svg"
      :viewBox="`0 0 100 ${height}`"
      preserveAspectRatio="none"
      :height="height"
      width="100%"
    >
      <rect
        v-for="(c, i) in counts"
        :key="i"
        :x="i * slot + 1"
        :y="bar(i).y"
        :width="Math.max(0.5, slot - 2)"
        :height="Math.max(c > 0 ? 1 : 0, bar(i).h)"
        rx="1"
        :class="['columns__bar', { 'columns__bar--active': active === i }]"
        :style="{ fill: color }"
      />
    </svg>
    <div class="columns__hits">
      <button
        v-for="(c, i) in counts"
        :key="i"
        type="button"
        class="columns__hit"
        :aria-label="`${label}, ${names[i]}: ${c}`"
        @pointerenter="active = i"
        @pointerleave="active = null"
        @focus="active = i"
        @blur="active = null"
      />
    </div>
    <div v-if="active !== null" class="columns__tooltip" role="status">
      <strong>{{ formatNumber(counts[active]!) }}</strong>
      <span>{{ names[active] }}</span>
    </div>
  </div>
</template>

<style scoped>
.columns {
  position: relative;
}

.columns__svg {
  display: block;
}

.columns__bar {
  shape-rendering: crispEdges;
}

.columns__bar--active {
  opacity: 0.7;
}

.columns__hits {
  position: absolute;
  inset: 0;
  display: flex;
}

.columns__hit {
  flex: 1;
  padding: 0;
  border: 0;
  background: transparent;
  cursor: default;
}

.columns__tooltip {
  position: absolute;
  top: -4px;
  right: 0;
  display: flex;
  gap: var(--space-2);
  padding: 2px var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-surface);
  font-size: var(--text-xs);
  pointer-events: none;
}

.columns__tooltip strong {
  font-family: var(--font-mono);
}

.columns__tooltip span {
  color: var(--color-text-muted);
}
</style>
