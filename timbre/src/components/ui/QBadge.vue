<script setup lang="ts">
withDefaults(defineProps<{ tone?: 'ok' | 'error' | 'neutral' }>(), { tone: 'neutral' })
</script>

<template>
  <span :class="['badge', `badge--${tone}`]"><slot /></span>
</template>

<style scoped>
.badge {
  display: inline-block;
  padding: 0 var(--space-2);
  border-radius: var(--radius-pill);
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.02em;
  text-transform: uppercase;
}

.badge--ok {
  background: var(--color-ok-bg);
  color: var(--color-ok);
}

.badge--error {
  background: var(--color-error-bg);
  color: var(--color-error);
}

.badge--neutral {
  background: var(--color-surface-raised);
  color: var(--color-text-muted);
}
</style>
