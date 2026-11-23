import { onBeforeUnmount, ref, shallowRef, watch, type Ref } from 'vue'
import { api } from '@/api/client'
import type { Frame, StreamEnd } from '@/api/types'

export type StreamState = 'connecting' | 'live' | 'ended' | 'failed' | 'error'

/**
 * Subscribe to a performance's frames over Server-Sent Events. Keeps the
 * previous and current frame plus the current frame's arrival time, which the
 * stage uses to interpolate between beats. `frame` (for the HUD) updates on
 * every beat; `prev`/`curr` are plain refs read from the render loop.
 */
export function usePerformanceStream(id: Ref<string>) {
  const state = ref<StreamState>('connecting')
  const frame = shallowRef<Frame | null>(null)
  const buffer = { prev: null as Frame | null, curr: null as Frame | null, arrivedAt: 0 }
  let source: EventSource | null = null

  function close() {
    source?.close()
    source = null
  }

  function open(performanceId: string) {
    close()
    state.value = 'connecting'
    buffer.prev = buffer.curr = null
    frame.value = null
    const es = new EventSource(api.streamUrl(performanceId))
    source = es
    es.addEventListener('frame', (event) => {
      const next = JSON.parse((event as MessageEvent<string>).data) as Frame
      buffer.prev = buffer.curr
      buffer.curr = next
      buffer.arrivedAt = performance.now()
      frame.value = next
      state.value = 'live'
    })
    es.addEventListener('end', (event) => {
      const end = JSON.parse((event as MessageEvent<string>).data) as StreamEnd
      state.value = end.status === 'failed' ? 'failed' : 'ended'
      close()
    })
    es.onerror = () => {
      // EventSource reconnects on its own while the connection is retryable.
      if (es.readyState === EventSource.CLOSED) state.value = 'error'
      else if (state.value === 'live') state.value = 'connecting'
    }
  }

  watch(id, open, { immediate: true })
  onBeforeUnmount(close)

  return { state, frame, buffer, close }
}
