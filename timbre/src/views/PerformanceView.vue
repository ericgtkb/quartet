<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { api } from '@/api/client'
import { VOICES, type Performance } from '@/api/types'
import CensusPanel from '@/components/CensusPanel.vue'
import StageCanvas from '@/components/StageCanvas.vue'
import QBadge from '@/components/ui/QBadge.vue'
import QButton from '@/components/ui/QButton.vue'
import { usePerformanceStream } from '@/stage/usePerformanceStream'

const props = defineProps<{ id: string }>()
const router = useRouter()

const info = ref<Performance | null>(null)
const error = ref<string | null>(null)
const id = computed(() => props.id)
const { state, frame, buffer } = usePerformanceStream(id)

const beatMs = computed(() => 1000 / (info.value?.beatsPerSecond ?? 15))
const arena = computed(() => Math.max(frame.value?.width ?? 100, frame.value?.height ?? 100))

const hud = computed(() => {
  const f = frame.value
  if (!f) return null
  const q = f.quartetlings
  const n = q.id.length || 1
  const mean = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / n
  return {
    beat: f.beat,
    quartetlings: q.id.length,
    singing: q.singing.reduce((a, b) => a + b, 0),
    inChord: f.stats.chordMembers,
    energy: mean(q.energy).toFixed(2),
    oldest: Math.max(0, ...q.age),
    generation: Math.max(0, ...q.generation),
  }
})

const stateTone = computed(() =>
  state.value === 'live' ? 'ok' : state.value === 'connecting' ? 'neutral' : 'error',
)

onMounted(async () => {
  try {
    info.value = await api.performance(props.id)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
})

async function stop() {
  try {
    await api.deletePerformance(props.id)
    await router.push({ name: 'performances' })
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}
</script>

<template>
  <div class="performance">
    <div class="performance__bar">
      <div>
        <h2 class="performance__title">
          {{ id }}
          <QBadge :tone="stateTone">{{ state }}</QBadge>
        </h2>
        <p v-if="info" class="performance__meta">
          {{ info.policy }}<template v-if="info.policyKind === 'checkpoint'"> (trained)</template> ·
          {{ info.slots }} quartetlings · {{ info.beatsPerSecond }} beats/s · seed
          <code>{{ info.seed }}</code
          >{{ info.evolve ? ' · evolving' : '' }}
        </p>
      </div>
      <QButton @click="stop">Stop</QButton>
    </div>

    <p v-if="error" class="performance__error" role="alert">{{ error }}</p>

    <div class="performance__stage">
      <StageCanvas :buffer="buffer" :beat-ms="beatMs" :size="arena" />
      <div v-if="hud" class="hud">
        <dl class="hud__stats">
          <dt>beat</dt>
          <dd>{{ hud.beat }}</dd>
          <dt>singing</dt>
          <dd>{{ hud.singing }} / {{ hud.quartetlings }}</dd>
          <dt>in a chord</dt>
          <dd>{{ hud.inChord }}</dd>
          <dt>mean energy</dt>
          <dd>{{ hud.energy }}</dd>
          <dt>oldest</dt>
          <dd>{{ hud.oldest }} beats</dd>
          <dt>generation</dt>
          <dd>{{ hud.generation }}</dd>
        </dl>
        <ul class="hud__legend">
          <li v-for="voice in VOICES" :key="voice">
            <span class="hud__swatch" :style="{ background: `var(--color-voice-${voice})` }" />
            {{ voice }}
          </li>
        </ul>
        <p class="hud__hint">ring: singing · wide ring: in a chord</p>
      </div>
    </div>

    <CensusPanel :id="id" />
  </div>
</template>

<style scoped>
.performance {
  display: grid;
  gap: var(--space-4);
}

.performance__bar {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--space-4);
}

.performance__title {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin: 0;
  font-family: var(--font-mono);
  font-size: var(--text-xl);
}

.performance__meta {
  margin: var(--space-1) 0 0;
  color: var(--color-text-muted);
  font-size: var(--text-sm);
}

.performance__error {
  margin: 0;
  padding: var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--color-error-bg);
  color: var(--color-error);
}

.performance__stage {
  position: relative;
  height: min(70vh, 720px);
}

.hud {
  position: absolute;
  top: var(--space-3);
  left: var(--space-3);
  display: grid;
  gap: var(--space-3);
  padding: var(--space-3);
  border-radius: var(--radius-sm);
  background: color-mix(in srgb, var(--color-surface) 85%, transparent);
  border: 1px solid var(--color-border);
  font-size: var(--text-xs);
  pointer-events: none;
}

.hud__stats {
  display: grid;
  grid-template-columns: max-content max-content;
  gap: var(--space-1) var(--space-3);
  margin: 0;
}

.hud__stats dt {
  color: var(--color-text-muted);
}

.hud__stats dd {
  margin: 0;
  font-family: var(--font-mono);
}

.hud__legend {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.hud__legend li {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.hud__swatch {
  width: 10px;
  height: 10px;
  border-radius: var(--radius-pill);
}

.hud__hint {
  margin: 0;
  color: var(--color-text-muted);
}
</style>
