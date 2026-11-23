<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue'
import { api } from '@/api/client'
import { VOICES, type Census, type Histogram } from '@/api/types'
import QColumns from '@/components/ui/QColumns.vue'
import QHeatmap from '@/components/ui/QHeatmap.vue'
import QLineChart from '@/components/ui/QLineChart.vue'
import QPanel from '@/components/ui/QPanel.vue'
import { formatNumber } from '@/components/ui/chart'

const props = withDefaults(defineProps<{ id: string; intervalMs?: number }>(), {
  intervalMs: 2000,
})

const census = shallowRef<Census | null>(null)
const error = ref<string | null>(null)
const loading = ref(false)
let timer: ReturnType<typeof setInterval> | undefined
let controller: AbortController | null = null

async function load() {
  if (loading.value) return
  loading.value = true
  controller = new AbortController()
  try {
    census.value = await api.census(props.id, controller.signal)
    error.value = null
  } catch (e) {
    if (!(e instanceof DOMException && e.name === 'AbortError')) {
      error.value = e instanceof Error ? e.message : String(e)
    }
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  void load()
  timer = setInterval(load, props.intervalMs)
})
onBeforeUnmount(() => {
  clearInterval(timer)
  controller?.abort()
})

const seriesColor = (v: number) => `var(--color-series-${VOICES[v]})`

const history = computed(() => {
  const h = census.value?.history
  if (!h) return null
  const perBeat = (xs: number[]) => xs.map((x, i) => x / (h.beats[i] || 1))
  return {
    beat: h.beat,
    energy: VOICES.map((voice, v) => ({
      label: voice,
      color: seriesColor(v),
      values: h.meanEnergy[v] ?? [],
    })),
    chord: [
      { label: 'in a chord', color: 'var(--color-series-1)', values: perBeat(h.chordMembers) },
    ],
    deaths: [
      {
        label: 'deaths',
        color: 'var(--color-series-1)',
        values: perBeat(h.deaths).map((d) => d * 1000),
      },
    ],
    drift: (
      [
        ['Mean tempo by voice', h.meanTempo],
        ['Mean pitch by voice', h.meanPitch],
        ['Mean dynamics by voice', h.meanDynamics],
        ['Mean generation by voice', h.meanGeneration],
      ] as const
    ).map(([title, series]) => ({
      title,
      series: VOICES.map((voice, v) => ({
        label: voice,
        color: seriesColor(v),
        values: series[v] ?? [],
      })),
    })),
    lineages: [{ label: 'lineages', color: 'var(--color-series-1)', values: h.lineages }],
    births: [
      {
        label: 'births',
        color: 'var(--color-series-1)',
        values: perBeat(h.births).map((b) => b * 1000),
      },
    ],
  }
})

const maxGeneration = computed(() =>
  Math.max(0, ...(census.value?.voices.map((v) => v.maxGeneration) ?? [])),
)

const traits = [
  { key: 'tempo', label: 'Tempo (max speed)' },
  { key: 'pitch', label: 'Pitch (hearing radius)' },
  { key: 'dynamics', label: 'Dynamics (body size)' },
  { key: 'energy', label: 'Energy' },
] as const

function binNames(h: Histogram) {
  const n = h.counts[0]?.length ?? 0
  const w = (h.max - h.min) / (n || 1)
  return Array.from(
    { length: n },
    (_, i) => `${formatNumber(h.min + i * w)}–${formatNumber(h.min + (i + 1) * w)}`,
  )
}

const fmt = (v: number) => v.toFixed(2)
</script>

<template>
  <QPanel title="Census">
    <template #actions>
      <span v-if="census" class="census__beat">beat {{ census.beat }}</span>
    </template>
    <p v-if="error" class="census__error" role="alert">{{ error }}</p>
    <p v-if="!census && !error" class="census__muted">Counting…</p>

    <div v-if="census" class="census" :class="{ 'census--loading': loading }">
      <div class="census__table">
        <table>
          <thead>
            <tr>
              <th scope="col">Voice</th>
              <th scope="col">Quartetlings</th>
              <th scope="col">Mean energy</th>
              <th scope="col">Mean age</th>
              <th scope="col">Tempo</th>
              <th scope="col">Pitch</th>
              <th scope="col">Dynamics</th>
              <th scope="col">Singing</th>
              <th scope="col">In a chord</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="v in census.voices" :key="v.voice">
              <th scope="row">
                <span class="census__swatch" :style="{ background: seriesColor(v.voice) }" />
                {{ VOICES[v.voice] }}
              </th>
              <td>{{ v.population }}</td>
              <td>{{ fmt(v.meanEnergy) }}</td>
              <td>{{ Math.round(v.meanAge) }}</td>
              <td>{{ fmt(v.tempo.mean) }} ± {{ fmt(v.tempo.std) }}</td>
              <td>{{ fmt(v.pitch.mean) }} ± {{ fmt(v.pitch.std) }}</td>
              <td>{{ fmt(v.dynamics.mean) }} ± {{ fmt(v.dynamics.std) }}</td>
              <td>{{ v.singing }}</td>
              <td>{{ v.inChord }}</td>
            </tr>
          </tbody>
        </table>
      </div>

      <div v-if="history" class="census__charts">
        <QLineChart
          title="Mean energy by voice"
          x-label="beat"
          :x="history.beat"
          :series="history.energy"
          :format="fmt"
        />
        <QLineChart
          title="Quartetlings in a chord, per beat"
          x-label="beat"
          :x="history.beat"
          :series="history.chord"
        />
        <QLineChart
          title="Deaths per 1,000 beats"
          x-label="beat"
          :x="history.beat"
          :series="history.deaths"
        />
      </div>

      <section v-if="history" class="census__evolution">
        <h3 class="trait__title">
          Evolution
          <span class="census__muted">
            generation up to {{ maxGeneration }} · {{ census.lineages }} founder lineages alive
          </span>
        </h3>
        <p class="census__muted census__note">
          Vacated slots are refilled with mutated offspring of well-fed, mature quartetlings of the
          same voice when any qualify, otherwise with random founders. Traits stay within the range
          policies were trained on.
        </p>
        <div class="census__charts">
          <QLineChart
            v-for="c in history.drift"
            :key="c.title"
            :title="c.title"
            x-label="beat"
            :x="history.beat"
            :series="c.series"
            :zero="false"
            :format="fmt"
          />
          <QLineChart
            title="Founder lineages alive"
            x-label="beat"
            :x="history.beat"
            :series="history.lineages"
          />
          <QLineChart
            title="Births per 1,000 beats"
            x-label="beat"
            :x="history.beat"
            :series="history.births"
          />
        </div>
      </section>

      <div class="census__lower">
        <div class="census__traits">
          <section v-for="t in traits" :key="t.key" class="trait">
            <h3 class="trait__title">
              {{ t.label }}
              <span class="census__muted">
                {{ formatNumber(census.histograms[t.key].min) }}–{{
                  formatNumber(census.histograms[t.key].max)
                }}
              </span>
            </h3>
            <div v-for="(voice, v) in VOICES" :key="voice" class="trait__row">
              <span class="trait__voice">{{ voice }}</span>
              <QColumns
                :label="`${t.label}, ${voice}`"
                :counts="census.histograms[t.key].counts[v] ?? []"
                :names="binNames(census.histograms[t.key])"
                :color="seriesColor(v)"
                :y-max="Math.max(...census.histograms[t.key].counts.flat())"
                :height="28"
              />
            </div>
          </section>
        </div>
        <section class="census__density">
          <h3 class="trait__title">Where they are <span class="census__muted">(+y up)</span></h3>
          <QHeatmap
            label="Quartetlings per cell of the arena"
            :columns="census.density.columns"
            :rows="census.density.rows"
            :counts="census.density.counts"
            :extra="{ label: 'singing', counts: census.density.singing }"
          />
        </section>
      </div>
    </div>
  </QPanel>
</template>

<style scoped>
.census {
  display: grid;
  gap: var(--space-5);
  transition: opacity 0.2s;
}

.census--loading {
  opacity: 0.85;
}

.census__beat,
.census__muted {
  color: var(--color-text-muted);
  font-size: var(--text-xs);
  font-weight: 400;
}

.census__error {
  margin: 0 0 var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--color-error-bg);
  color: var(--color-error);
  font-size: var(--text-sm);
}

.census__table {
  overflow-x: auto;
}

.census__table table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--text-xs);
}

.census__table th,
.census__table td {
  padding: var(--space-1) var(--space-2);
  border-bottom: 1px solid var(--color-grid);
  text-align: right;
  white-space: nowrap;
}

.census__table td {
  font-family: var(--font-mono);
}

.census__table th[scope='row'],
.census__table thead th:first-child {
  text-align: left;
}

.census__table thead th {
  color: var(--color-text-muted);
  font-weight: 600;
}

.census__swatch {
  display: inline-block;
  width: 8px;
  height: 8px;
  margin-right: var(--space-1);
  border-radius: var(--radius-pill);
}

.census__note {
  margin: 0 0 var(--space-3);
}

.census__charts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 280px), 1fr));
  gap: var(--space-4);
}

.census__lower {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 320px), 1fr));
  gap: var(--space-5);
  align-items: start;
}

.census__traits {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 220px), 1fr));
  gap: var(--space-4);
}

.trait__title {
  margin: 0 0 var(--space-2);
  font-size: var(--text-sm);
}

.trait__row {
  display: grid;
  grid-template-columns: 56px 1fr;
  align-items: end;
  gap: var(--space-2);
  margin-bottom: 2px;
}

.trait__voice {
  color: var(--color-text-muted);
  font-size: var(--text-xs);
}

.census__density {
  max-width: 360px;
}
</style>
