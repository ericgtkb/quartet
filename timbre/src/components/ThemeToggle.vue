<script setup lang="ts">
import { computed } from 'vue'
import QButton from '@/components/ui/QButton.vue'
import { useTheme } from '@/composables/useTheme'

const { theme, toggle } = useTheme()
const next = computed(() => (theme.value === 'dark' ? 'light' : 'dark'))
</script>

<template>
  <QButton
    :aria-label="`Switch to ${next} theme`"
    :title="`Switch to ${next} theme`"
    @click="toggle"
  >
    <svg v-if="theme === 'dark'" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
      <circle cx="12" cy="12" r="4.5" fill="currentColor" />
      <g stroke="currentColor" stroke-width="2" stroke-linecap="round">
        <path
          d="M12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"
        />
      </g>
    </svg>
    <svg v-else viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
      <path fill="currentColor" d="M20.5 14.5A8.5 8.5 0 0 1 9.5 3.5a8.5 8.5 0 1 0 11 11Z" />
    </svg>
    <span>{{ next === 'dark' ? 'Dark' : 'Light' }}</span>
  </QButton>
</template>
