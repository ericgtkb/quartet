<script setup lang="ts">
import { computed, ref } from 'vue'

/**
 * A grid of counts on a single-hue sequential ramp (surface → series color).
 * Row 0 is drawn at the bottom, so +y points up. Each cell is a hover/focus
 * target with its count in the tooltip.
 */
const props = defineProps<{
  label: string
  columns: number
  rows: number
  /** counts[row * columns + column] */
  counts: number[]
  /** Optional second count per cell shown in the tooltip, e.g. singing. */
  extra?: { label: string; counts: number[] }
}>()

const active = ref<number | null>(null)
const max = computed(() => Math.max(1, ...props.counts))
const cells = computed(() => {
  const out: { index: number; row: number; column: number }[] = []
  for (let row = props.rows - 1; row >= 0; row--) {
    for (let column = 0; column < props.columns; column++) {
      out.push({ index: row * props.columns + column, row, column })
    }
  }
  return out
})

function fill(count: number) {
  if (count === 0) return 'var(--color-surface-raised)'
  // Square root so a few dense cells don't wash out the rest.
  const pct = Math.round(15 + 85 * Math.sqrt(count / max.value))
  return `color-mix(in oklab, var(--color-series-1) ${pct}%, var(--color-surface))`
}
</script>

<template>
  <div class="heatmap">
    <div
      class="heatmap__grid"
      role="grid"
      :aria-label="label"
      :style="{ gridTemplateColumns: `repeat(${columns}, 1fr)` }"
    >
      <button
        v-for="c in cells"
        :key="c.index"
        type="button"
        class="heatmap__cell"
        :style="{ background: fill(counts[c.index] ?? 0) }"
        :aria-label="`cell ${c.column}, ${c.row}: ${counts[c.index] ?? 0}`"
        @pointerenter="active = c.index"
        @pointerleave="active = null"
        @focus="active = c.index"
        @blur="active = null"
      />
    </div>
    <div class="heatmap__foot">
      <span class="heatmap__scale">
        0
        <span class="heatmap__ramp" />
        {{ max }}
      </span>
      <span v-if="active !== null" role="status">
        <strong>{{ counts[active] ?? 0 }}</strong> here<template v-if="extra">
          · <strong>{{ extra.counts[active] ?? 0 }}</strong> {{ extra.label }}</template
        >
      </span>
    </div>
  </div>
</template>

<style scoped>
.heatmap__grid {
  display: grid;
  gap: 1px;
  aspect-ratio: 1;
  background: var(--color-surface);
}

.heatmap__cell {
  min-width: 0;
  padding: 0;
  border: 0;
  border-radius: 1px;
  cursor: default;
}

.heatmap__cell:hover,
.heatmap__cell:focus-visible {
  outline: 2px solid var(--color-text);
  outline-offset: -1px;
}

.heatmap__foot {
  display: flex;
  justify-content: space-between;
  gap: var(--space-2);
  margin-top: var(--space-1);
  color: var(--color-text-muted);
  font-size: var(--text-xs);
}

.heatmap__foot strong {
  color: var(--color-text);
  font-family: var(--font-mono);
}

.heatmap__scale {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  font-family: var(--font-mono);
}

.heatmap__ramp {
  width: 64px;
  height: 8px;
  border-radius: 2px;
  background: linear-gradient(
    to right,
    color-mix(in oklab, var(--color-series-1) 15%, var(--color-surface)),
    var(--color-series-1)
  );
}
</style>
