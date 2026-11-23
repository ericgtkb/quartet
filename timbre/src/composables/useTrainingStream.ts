import { onBeforeUnmount, ref, shallowRef, triggerRef, watch, type Ref } from 'vue'
import { api } from '@/api/client'
import type { TrainingMetrics, TrainingStatus, TrainingStreamEnd } from '@/api/types'

export type TrainingStreamState = 'connecting' | 'live' | 'ended' | 'error'

/**
 * Follow a training run's metrics over Server-Sent Events. rhythm replays the
 * history first, then streams one event per PPO update, then "end" with the
 * final status. `metrics` is a shallow ref mutated in place (triggerRef),
 * so long runs don't copy the array on every update.
 */
export function useTrainingStream(id: Ref<string>) {
  const state = ref<TrainingStreamState>('connecting')
  const status = ref<TrainingStatus | null>(null)
  const metrics = shallowRef<TrainingMetrics[]>([])
  let source: EventSource | null = null

  function close() {
    source?.close()
    source = null
  }

  function open(runId: string) {
    close()
    state.value = 'connecting'
    status.value = null
    metrics.value = []
    const es = new EventSource(api.trainingStreamUrl(runId))
    source = es
    es.addEventListener('metrics', (event) => {
      metrics.value.push(JSON.parse((event as MessageEvent<string>).data) as TrainingMetrics)
      triggerRef(metrics)
      state.value = 'live'
    })
    es.addEventListener('end', (event) => {
      status.value = (JSON.parse((event as MessageEvent<string>).data) as TrainingStreamEnd).status
      state.value = 'ended'
      close()
    })
    es.onerror = () => {
      if (es.readyState === EventSource.CLOSED) state.value = 'error'
      else if (state.value === 'live') {
        // The browser reconnects and rhythm replays the history: start over.
        metrics.value = []
        state.value = 'connecting'
      }
    }
  }

  watch(id, open, { immediate: true })
  onBeforeUnmount(close)

  return { state, status, metrics, close }
}
