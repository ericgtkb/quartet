import { createRouter, createWebHistory } from 'vue-router'
import EnsembleView from '@/views/EnsembleView.vue'
import PerformancesView from '@/views/PerformancesView.vue'
import TrainingView from '@/views/TrainingView.vue'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    { path: '/', name: 'performances', component: PerformancesView },
    {
      path: '/performances/:id',
      name: 'performance',
      // Lazy: keeps three.js out of the initial bundle.
      component: () => import('@/views/PerformanceView.vue'),
      props: true,
    },
    { path: '/training', name: 'training', component: TrainingView },
    {
      path: '/training/:id',
      name: 'training-run',
      component: () => import('@/views/TrainingRunView.vue'),
      props: true,
    },
    { path: '/ensemble', name: 'ensemble', component: EnsembleView },
  ],
})

export default router
