import { afterEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import { mount } from '@vue/test-utils'
import { usePerformanceStream } from '@/stage/usePerformanceStream'
import { makeFrame } from './fixtures'

class FakeEventSource {
  static CLOSED = 2
  static last: FakeEventSource | null = null
  readyState = 1
  onerror: (() => void) | null = null
  listeners = new Map<string, (e: MessageEvent) => void>()
  constructor(readonly url: string) {
    FakeEventSource.last = this
  }
  addEventListener(type: string, fn: (e: MessageEvent) => void) {
    this.listeners.set(type, fn)
  }
  emit(type: string, data: unknown) {
    this.listeners.get(type)?.(new MessageEvent(type, { data: JSON.stringify(data) }))
  }
  close() {
    this.readyState = FakeEventSource.CLOSED
  }
}

afterEach(() => vi.unstubAllGlobals())

function mountStream(id = 'p1') {
  let result!: ReturnType<typeof usePerformanceStream>
  const wrapper = mount(
    defineComponent({
      setup() {
        result = usePerformanceStream(ref(id))
        return () => h('div')
      },
    }),
  )
  return { wrapper, stream: result }
}

describe('usePerformanceStream', () => {
  it('buffers the previous and current frame', () => {
    vi.stubGlobal('EventSource', FakeEventSource)
    const { stream } = mountStream()
    const es = FakeEventSource.last!
    expect(es.url).toBe('/api/performances/p1/stream')
    expect(stream.state.value).toBe('connecting')

    es.emit('frame', makeFrame(1, [10]))
    es.emit('frame', makeFrame(2, [20]))
    expect(stream.state.value).toBe('live')
    expect(stream.buffer.prev?.beat).toBe(1)
    expect(stream.buffer.curr?.beat).toBe(2)
    expect(stream.frame.value?.beat).toBe(2)
  })

  it('ends on the end event and closes on unmount', () => {
    vi.stubGlobal('EventSource', FakeEventSource)
    const { wrapper, stream } = mountStream()
    const es = FakeEventSource.last!
    es.emit('end', { status: 'failed' })
    expect(stream.state.value).toBe('failed')
    expect(es.readyState).toBe(FakeEventSource.CLOSED)

    const second = mountStream('p2')
    const es2 = FakeEventSource.last!
    second.wrapper.unmount()
    expect(es2.readyState).toBe(FakeEventSource.CLOSED)
    wrapper.unmount()
  })
})
