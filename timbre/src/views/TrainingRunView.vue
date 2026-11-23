<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { api } from '@/api/client'
import type { TrainingMetrics, TrainingRun } from '@/api/types'
import QBadge from '@/components/ui/QBadge.vue'
import QButton from '@/components/ui/QButton.vue'
import QLineChart from '@/components/ui/QLineChart.vue'
import QPanel from '@/components/ui/QPanel.vue'
import { useTrainingStream } from '@/composables/useTrainingStream'
import { statusTone } from '@/views/trainingStatus'

const props = defineProps<{ id: string }>()
const router = useRouter()
const run = ref<TrainingRun | null>(null)
const error = ref<string | null>(null)
const stopping = ref(false)
const id = computed(() => props.id)
const { state, status, metrics } = useTrainingStream(id)

async function load() {
  try {
    run.value = await api.trainingRun(props.id)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}
onMounted(load)
// Refresh the run (status, error) when its stream ends.
watch(status, (s) => s && load())

const currentStatus = computed(() => status.value ?? run.value?.status ?? null)
const latest = computed(() => metrics.value[metrics.value.length - 1] ?? run.value?.latest)
const progress = computed(() =>
  run.value && latest.value
    ? Math.min(1, latest.value.globalStep / run.value.config.totalSteps)
    : 0,
)

/** Millions of agent-steps on the x axis. */
const x = computed(() => metrics.value.map((m) => m.globalStep / 1e6))
function line(label: string, pick: (m: TrainingMetrics) => number) {
  return [{ label, color: 'var(--color-series-1)', values: metrics.value.map(pick) }]
}

const charts = computed(() => [
  {
    title: 'Deaths per 1,000 beats (64 quartetlings)',
    series: line('deaths', (m) => m.deathsPer1kBeats),
  },
  {
    title: 'Lifespan of lives that ended (beats)',
    series: line('lifespan', (m) => m.episodeLength),
  },
  { title: 'Mean energy', series: line('energy', (m) => m.meanEnergy) },
  {
    title: 'Chord rate (share of quartetling-beats)',
    series: line('chord rate', (m) => m.chordRate),
  },
  { title: 'Motes eaten per 1,000 beats', series: line('motes', (m) => m.motesPer1kBeats) },
  { title: 'Reward per step', series: line('reward', (m) => m.rewardPerStep), zero: false },
  { title: 'Value loss', series: line('value loss', (m) => m.valueLoss) },
  { title: 'Policy loss', series: line('policy loss', (m) => m.policyLoss), zero: false },
  { title: 'Entropy', series: line('entropy', (m) => m.entropy), zero: false },
  { title: 'Approximate KL', series: line('approx KL', (m) => m.approxKl) },
  { title: 'Explained variance', series: line('explained variance', (m) => m.explainedVariance) },
  { title: 'Agent-steps per second', series: line('steps/s', (m) => m.stepsPerSecond) },
])

async function stop() {
  stopping.value = true
  try {
    run.value = await api.stopTraining(props.id)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    stopping.value = false
  }
}

function perform() {
  void router.push({ name: 'performances', query: { policy: props.id } })
}
</script>

<template>
  <div class="run">
    <div class="run__bar">
      <div>
        <h2 class="run__title">
          {{ id }}
          <QBadge :tone="statusTone(currentStatus)">{{ currentStatus ?? state }}</QBadge>
        </h2>
        <p v-if="run" class="run__meta">
          {{ run.config.worlds }} worlds × {{ run.config.slots }} quartetlings · seed
          <code>{{ run.config.seed }}</code> · {{ (run.config.totalSteps / 1e6).toFixed(1) }}M
          agent-steps
          <template v-if="latest">
            · {{ Math.round(latest.stepsPerSecond).toLocaleString('en') }} steps/s</template
          >
        </p>
      </div>
      <div class="run__actions">
        <QButton v-if="latest" @click="perform">Perform with it</QButton>
        <QButton v-if="currentStatus === 'running'" :disabled="stopping" @click="stop">
          {{ stopping ? 'Stopping…' : 'Stop' }}
        </QButton>
      </div>
    </div>

    <p v-if="error" class="run__error" role="alert">{{ error }}</p>
    <p v-if="run?.error" class="run__error" role="alert">{{ run.error }}</p>

    <div
      class="run__progress"
      role="progressbar"
      :aria-valuenow="Math.round(progress * 100)"
      aria-valuemin="0"
      aria-valuemax="100"
    >
      <span :style="{ width: `${progress * 100}%` }" />
    </div>

    <QPanel>
      <p class="run__note">
        One point per PPO update. A life's undiscounted return is 10 × (final − starting energy),
        minus 1 on death, so every death scores −11: lifespan and deaths show progress better than
        return.
      </p>
      <div class="run__charts">
        <QLineChart
          v-for="c in charts"
          :key="c.title"
          :title="c.title"
          x-label="M steps"
          :x="x"
          :series="c.series"
          :zero="c.zero ?? true"
        />
      </div>
    </QPanel>
  </div>
</template>

<style scoped>
.run {
  display: grid;
  gap: var(--space-4);
}

.run__bar {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--space-4);
}

.run__title {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin: 0;
  font-family: var(--font-mono);
  font-size: var(--text-lg);
}

.run__meta,
.run__note {
  margin: var(--space-1) 0 0;
  color: var(--color-text-muted);
  font-size: var(--text-sm);
}

.run__note {
  margin: 0 0 var(--space-4);
}

.run__actions {
  display: flex;
  gap: var(--space-2);
}

.run__error {
  margin: 0;
  padding: var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--color-error-bg);
  color: var(--color-error);
}

.run__progress {
  height: 4px;
  border-radius: var(--radius-pill);
  background: color-mix(in oklab, var(--color-series-1) 20%, var(--color-surface));
  overflow: hidden;
}

.run__progress span {
  display: block;
  height: 100%;
  background: var(--color-series-1);
}

.run__charts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 300px), 1fr));
  gap: var(--space-5) var(--space-4);
}
</style>
