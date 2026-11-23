<script setup lang="ts">
withDefaults(defineProps<{ variant?: 'primary' | 'ghost' }>(), { variant: 'ghost' })
</script>

<template>
  <button type="button" :class="['button', `button--${variant}`]">
    <slot />
  </button>
</template>

<style scoped>
.button {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-3);
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border);
  font: inherit;
  font-size: var(--text-sm);
  cursor: pointer;
}

.button:disabled {
  cursor: progress;
  opacity: 0.6;
}

.button--primary {
  background: var(--color-accent);
  border-color: var(--color-accent);
  color: var(--color-accent-contrast);
}

.button--ghost {
  background: transparent;
  color: var(--color-text);
}

.button--ghost:hover:not(:disabled) {
  background: var(--color-surface-raised);
}
</style>
