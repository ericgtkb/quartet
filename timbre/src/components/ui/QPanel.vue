<script setup lang="ts">
defineProps<{ title?: string }>()
</script>

<template>
  <section class="panel">
    <header v-if="title || $slots.actions" class="panel__header">
      <h2 v-if="title" class="panel__title">{{ title }}</h2>
      <div class="panel__actions"><slot name="actions" /></div>
    </header>
    <slot />
  </section>
</template>

<style scoped>
.panel {
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  padding: var(--space-4);
}

.panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  margin-bottom: var(--space-3);
}

.panel__title {
  margin: 0;
  font-size: var(--text-md);
  font-weight: 600;
}

.panel__actions {
  display: flex;
  gap: var(--space-2);
}
</style>
