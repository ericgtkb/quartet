<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, version as vueVersion } from 'vue'
import { api } from '@/api/client'
import type { Member } from '@/api/types'
import MemberCard from '@/components/MemberCard.vue'
import QButton from '@/components/ui/QButton.vue'

const self: Member = {
  name: 'timbre',
  status: 'ok',
  version: __TIMBRE_VERSION__,
  runtime: `Vue ${vueVersion}`,
  details: {},
}

const remote = ref<Member[]>([])
const error = ref<string | null>(null)
const loading = ref(false)
let controller: AbortController | null = null

const members = computed(() => [self, ...remote.value])

async function load() {
  controller?.abort()
  controller = new AbortController()
  loading.value = true
  try {
    remote.value = (await api.ensemble(controller.signal)).members
    error.value = null
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') return
    remote.value = []
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

onMounted(load)
onBeforeUnmount(() => controller?.abort())
</script>

<template>
  <div class="ensemble">
    <div class="ensemble__intro">
      <div>
        <h2 class="ensemble__title">The ensemble</h2>
        <p class="ensemble__lede">
          Each card is one service. timbre asks rhythm, which asks harmony and melody over gRPC.
        </p>
      </div>
      <QButton :disabled="loading" @click="load">{{ loading ? 'Checking…' : 'Refresh' }}</QButton>
    </div>

    <p v-if="error" class="ensemble__error" role="alert">rhythm is unreachable: {{ error }}</p>

    <div class="ensemble__grid">
      <MemberCard v-for="member in members" :key="member.name" :member="member" />
    </div>
  </div>
</template>

<style scoped>
.ensemble__intro {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--space-4);
  margin-bottom: var(--space-5);
}

.ensemble__title {
  margin: 0;
  font-size: var(--text-xl);
}

.ensemble__lede {
  margin: var(--space-1) 0 0;
  color: var(--color-text-muted);
}

.ensemble__error {
  margin: 0 0 var(--space-4);
  padding: var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--color-error-bg);
  color: var(--color-error);
}

.ensemble__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(100%, 260px), 1fr));
  gap: var(--space-4);
}
</style>
