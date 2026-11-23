<script setup lang="ts">
import ThemeToggle from '@/components/ThemeToggle.vue'
</script>

<template>
  <header class="app-header">
    <div>
      <h1 class="app-header__title">Quartet</h1>
      <p class="app-header__subtitle">quartetlings, learning to sing together</p>
    </div>
    <nav class="app-header__nav">
      <RouterLink :to="{ name: 'performances' }">Stage</RouterLink>
      <RouterLink :to="{ name: 'training' }">Training</RouterLink>
      <RouterLink :to="{ name: 'ensemble' }">Ensemble</RouterLink>
      <ThemeToggle />
    </nav>
  </header>
  <main class="app-main">
    <RouterView />
  </main>
</template>

<style scoped>
.app-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  padding: var(--space-4);
  border-bottom: 1px solid var(--color-border);
  background: var(--color-surface);
}

.app-header__title {
  margin: 0;
  font-size: var(--text-lg);
  letter-spacing: 0.02em;
}

.app-header__subtitle {
  margin: 0;
  color: var(--color-text-muted);
  font-size: var(--text-sm);
}

.app-header__nav {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  font-size: var(--text-sm);
}

.app-header__nav a {
  color: var(--color-text-muted);
  text-decoration: none;
}

.app-header__nav a.router-link-exact-active {
  color: var(--color-text);
  font-weight: 600;
}

.app-main {
  max-width: 1200px;
  margin: 0 auto;
  padding: var(--space-5) var(--space-4);
}
</style>
