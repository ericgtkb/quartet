<script setup lang="ts">
import { computed } from 'vue'
import QBadge from '@/components/ui/QBadge.vue'
import QPanel from '@/components/ui/QPanel.vue'
import type { Member } from '@/api/types'

const props = defineProps<{ member: Member }>()

// Static presentation metadata for the four components.
const ROLES: Record<string, { role: string; language: string; color: string }> = {
  timbre: { role: 'stage', language: 'TypeScript', color: 'var(--color-voice-soprano)' },
  rhythm: { role: 'conductor', language: 'Go', color: 'var(--color-voice-tenor)' },
  harmony: { role: 'world engine', language: 'Rust', color: 'var(--color-voice-alto)' },
  melody: { role: 'learning', language: 'Python', color: 'var(--color-voice-bass)' },
}

const meta = computed(
  () =>
    ROLES[props.member.name] ?? {
      role: 'unknown',
      language: '',
      color: 'var(--color-text-muted)',
    },
)
const details = computed(() => Object.entries(props.member.details))
</script>

<template>
  <QPanel class="member" :style="{ '--member-color': meta.color }">
    <div class="member__head">
      <div>
        <h3 class="member__name">{{ member.name }}</h3>
        <p class="member__role">{{ meta.role }} · {{ meta.language }}</p>
      </div>
      <QBadge :tone="member.status === 'ok' ? 'ok' : 'error'">{{ member.status }}</QBadge>
    </div>

    <dl class="member__facts">
      <template v-if="member.version">
        <dt>version</dt>
        <dd>{{ member.version }}</dd>
      </template>
      <template v-if="member.runtime">
        <dt>runtime</dt>
        <dd>{{ member.runtime }}</dd>
      </template>
      <template v-for="[key, value] in details" :key="key">
        <dt>{{ key }}</dt>
        <dd>{{ value }}</dd>
      </template>
    </dl>
    <p v-if="member.error" class="member__error">{{ member.error }}</p>
  </QPanel>
</template>

<style scoped>
.member {
  border-top: 3px solid var(--member-color);
}

.member__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-3);
}

.member__name {
  margin: 0;
  font-size: var(--text-lg);
}

.member__role {
  margin: 0;
  color: var(--color-text-muted);
  font-size: var(--text-sm);
}

.member__facts {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: var(--space-1) var(--space-3);
  margin: var(--space-4) 0 0;
  font-size: var(--text-sm);
}

.member__facts dt {
  color: var(--color-text-muted);
}

.member__facts dd {
  margin: 0;
  font-family: var(--font-mono);
  overflow-wrap: anywhere;
}

.member__error {
  margin: var(--space-3) 0 0;
  color: var(--color-error);
  font-size: var(--text-sm);
  overflow-wrap: anywhere;
}
</style>
