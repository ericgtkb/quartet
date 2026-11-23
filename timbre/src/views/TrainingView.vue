<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { api } from '@/api/client'
import type { TrainingRun } from '@/api/types'
import QBadge from '@/components/ui/QBadge.vue'
import QButton from '@/components/ui/QButton.vue'
import QPanel from '@/components/ui/QPanel.vue'
import { statusTone } from '@/views/trainingStatus'

const router = useRouter()
const runs = ref<TrainingRun[]>([])
const error = ref<string | null>(null)
const starting = ref(false)
const form = ref({ millions: 50, worlds: 16, slots: 64, seed: 1 })

function message(e: unknown) {
  return e instanceof Error ? e.message : String(e)
}

async function refresh() {
  try {
    runs.value = (await api.trainingRuns()).runs
    error.value = null
  } catch (e) {
    error.value = message(e)
  }
}

async function start() {
  starting.value = true
  try {
    const run = await api.startTraining({
      totalSteps: Math.round(form.value.millions * 1e6),
      worlds: form.value.worlds,
      slots: form.value.slots,
      seed: form.value.seed,
    })
    await router.push({ name: 'training-run', params: { id: run.id } })
  } catch (e) {
    error.value = message(e)
  } finally {
    starting.value = false
  }
}

const millions = (n: number) => `${(n / 1e6).toFixed(1)}M`

onMounted(refresh)
</script>

<template>
  <div class="training">
    <div>
      <h2 class="training__title">Training</h2>
      <p class="training__lede">
        PPO in melody against a rehearsal in harmony: worlds × slots quartetlings share one policy.
        Each run's checkpoint becomes a policy you can start a performance with.
      </p>
    </div>

    <p v-if="error" class="training__error" role="alert">{{ error }}</p>

    <div class="training__layout">
      <QPanel title="Start a run">
        <form class="form" @submit.prevent="start">
          <label class="form__field">
            <span>Agent-steps (millions)</span>
            <input
              v-model.number="form.millions"
              name="millions"
              type="number"
              min="0.1"
              step="0.1"
            />
          </label>
          <label class="form__field">
            <span>Worlds</span>
            <input v-model.number="form.worlds" name="worlds" type="number" min="1" max="1024" />
          </label>
          <label class="form__field">
            <span>Quartetlings per world</span>
            <input v-model.number="form.slots" name="slots" type="number" min="4" step="4" />
          </label>
          <label class="form__field">
            <span>Seed</span>
            <input v-model.number="form.seed" name="seed" type="number" min="1" />
          </label>
          <QButton variant="primary" type="submit" :disabled="starting">
            {{ starting ? 'Starting…' : 'Start training' }}
          </QButton>
        </form>
      </QPanel>

      <QPanel title="Runs">
        <template #actions><QButton @click="refresh">Refresh</QButton></template>
        <p v-if="runs.length === 0" class="training__empty">No training runs yet.</p>
        <ul v-else class="list">
          <li v-for="r in runs" :key="r.id">
            <RouterLink class="list__item" :to="{ name: 'training-run', params: { id: r.id } }">
              <span class="list__id">{{ r.id }}</span>
              <span>
                {{ millions(r.latest?.globalStep ?? 0) }} /
                {{ millions(r.config.totalSteps) }} steps
                <template v-if="r.latest">
                  · {{ r.latest.deathsPer1kBeats.toFixed(0) }} deaths/1k beats</template
                >
              </span>
              <QBadge :tone="statusTone(r.status)">{{ r.status }}</QBadge>
            </RouterLink>
          </li>
        </ul>
      </QPanel>
    </div>
  </div>
</template>

<style scoped>
.training {
  display: grid;
  gap: var(--space-5);
}

.training__title {
  margin: 0;
  font-size: var(--text-xl);
}

.training__lede,
.training__empty {
  margin: var(--space-1) 0 0;
  color: var(--color-text-muted);
}

.training__error {
  margin: 0;
  padding: var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--color-error-bg);
  color: var(--color-error);
}

.training__layout {
  display: grid;
  grid-template-columns: minmax(min(100%, 280px), 1fr) minmax(min(100%, 320px), 2fr);
  gap: var(--space-4);
  align-items: start;
}

@media (max-width: 720px) {
  .training__layout {
    grid-template-columns: 1fr;
  }
}

.form {
  display: grid;
  gap: var(--space-3);
  justify-items: start;
}

.form__field {
  display: grid;
  gap: var(--space-1);
  width: 100%;
  font-size: var(--text-sm);
}

.list {
  display: grid;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.list__item {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  color: inherit;
  text-decoration: none;
  font-size: var(--text-sm);
}

.list__item:hover {
  background: var(--color-surface-raised);
}

.list__id {
  font-family: var(--font-mono);
  font-weight: 600;
}
</style>
