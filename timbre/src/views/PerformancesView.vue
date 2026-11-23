<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api } from '@/api/client'
import type { Performance, Policy } from '@/api/types'
import QBadge from '@/components/ui/QBadge.vue'
import QButton from '@/components/ui/QButton.vue'
import QPanel from '@/components/ui/QPanel.vue'

const router = useRouter()
const route = useRoute()
const policies = ref<Policy[]>([])
const performances = ref<Performance[]>([])
const error = ref<string | null>(null)
const starting = ref(false)

// ?policy=<run id> preselects a trained policy (from a training run's page).
const requested = typeof route.query.policy === 'string' ? route.query.policy : null
const form = ref({
  policy: requested ?? 'choir',
  slots: 64,
  beatsPerSecond: 15,
  seed: '',
  evolve: true,
})
const builtins = computed(() => policies.value.filter((p) => p.kind === 'builtin'))
const trained = computed(() => policies.value.filter((p) => p.kind === 'checkpoint'))

function message(e: unknown) {
  return e instanceof Error ? e.message : String(e)
}

async function refresh() {
  try {
    const [p, list] = await Promise.all([api.policies(), api.performances()])
    policies.value = p.policies
    performances.value = list.performances
    error.value = null
  } catch (e) {
    error.value = message(e)
  }
}

async function start() {
  starting.value = true
  try {
    const seed = form.value.seed.trim()
    const created = await api.createPerformance({
      policy: form.value.policy,
      slots: form.value.slots,
      beatsPerSecond: form.value.beatsPerSecond,
      evolve: form.value.evolve,
      ...(seed ? { seed: Number(seed) } : {}),
    })
    await router.push({ name: 'performance', params: { id: created.id } })
  } catch (e) {
    error.value = message(e)
  } finally {
    starting.value = false
  }
}

onMounted(refresh)
</script>

<template>
  <div class="performances">
    <div>
      <h2 class="performances__title">Performances</h2>
      <p class="performances__lede">
        Each performance is a live world in harmony, conducted by rhythm, beat by beat.
      </p>
    </div>

    <p v-if="error" class="performances__error" role="alert">{{ error }}</p>

    <div class="performances__layout">
      <QPanel title="Start a performance">
        <form class="form" @submit.prevent="start">
          <label class="form__field">
            <span>Policy</span>
            <select v-model="form.policy" name="policy">
              <optgroup label="Builtin">
                <option v-for="p in builtins" :key="p.name" :value="p.name">{{ p.name }}</option>
              </optgroup>
              <optgroup v-if="trained.length" label="Trained">
                <option v-for="p in trained" :key="p.name" :value="p.name">
                  {{ p.name }}{{ p.trainingStatus === 'running' ? ' (training)' : '' }}
                </option>
              </optgroup>
            </select>
            <small>{{ policies.find((p) => p.name === form.policy)?.description }}</small>
          </label>
          <label class="form__field">
            <span>Quartetlings</span>
            <input
              v-model.number="form.slots"
              name="slots"
              type="number"
              min="4"
              max="1024"
              step="4"
            />
          </label>
          <label class="form__field">
            <span>Beats per second</span>
            <input v-model.number="form.beatsPerSecond" name="bps" type="number" min="1" max="60" />
          </label>
          <label class="form__field">
            <span>Seed <small>(optional)</small></span>
            <input v-model="form.seed" name="seed" inputmode="numeric" placeholder="random" />
          </label>
          <label class="form__check">
            <input v-model="form.evolve" name="evolve" type="checkbox" />
            <span>Evolve: refill slots with offspring of well-fed quartetlings</span>
          </label>
          <QButton variant="primary" type="submit" :disabled="starting">
            {{ starting ? 'Starting…' : 'Start' }}
          </QButton>
        </form>
      </QPanel>

      <QPanel title="Live">
        <template #actions><QButton @click="refresh">Refresh</QButton></template>
        <p v-if="performances.length === 0" class="performances__empty">Nothing playing yet.</p>
        <ul v-else class="list">
          <li v-for="p in performances" :key="p.id">
            <RouterLink class="list__item" :to="{ name: 'performance', params: { id: p.id } }">
              <span class="list__id">{{ p.id }}</span>
              <span>{{ p.policy }} · {{ p.slots }} quartetlings · beat {{ p.beat }}</span>
              <QBadge :tone="p.status === 'playing' ? 'ok' : 'error'">{{ p.status }}</QBadge>
            </RouterLink>
          </li>
        </ul>
      </QPanel>
    </div>
  </div>
</template>

<style scoped>
.performances {
  display: grid;
  gap: var(--space-5);
}

.performances__title {
  margin: 0;
  font-size: var(--text-xl);
}

.performances__lede,
.performances__empty {
  margin: var(--space-1) 0 0;
  color: var(--color-text-muted);
}

.performances__error {
  margin: 0;
  padding: var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--color-error-bg);
  color: var(--color-error);
}

.performances__layout {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 320px), 1fr));
  gap: var(--space-4);
  align-items: start;
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

.form__check {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
}

.form__field small {
  color: var(--color-text-muted);
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
