import { afterEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import { createMemoryHistory, createRouter } from 'vue-router'
import { useTrainingStream } from '@/composables/useTrainingStream'
import TrainingView from '@/views/TrainingView.vue'

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

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status })

describe('useTrainingStream', () => {
  it('collects metrics and ends with the run status', () => {
    vi.stubGlobal('EventSource', FakeEventSource)
    let stream!: ReturnType<typeof useTrainingStream>
    mount(
      defineComponent({
        setup() {
          stream = useTrainingStream(ref('run-1'))
          return () => h('div')
        },
      }),
    )
    const es = FakeEventSource.last!
    expect(es.url).toBe('/api/training-runs/run-1/stream')
    es.emit('metrics', { update: 1, globalStep: 100 })
    es.emit('metrics', { update: 2, globalStep: 200 })
    expect(stream.state.value).toBe('live')
    expect(stream.metrics.value.map((m) => m.update)).toEqual([1, 2])
    es.emit('end', { status: 'stopped' })
    expect(stream.status.value).toBe('stopped')
    expect(stream.state.value).toBe('ended')
    expect(es.readyState).toBe(FakeEventSource.CLOSED)
  })
})

describe('TrainingView', () => {
  it('lists runs and starts one', async () => {
    const fetchMock = vi.fn<typeof fetch>(async (input, init) => {
      if (init?.method === 'POST') return json({ id: 'ppo-new' }, 201)
      return json({
        runs: [
          {
            id: 'ppo-old',
            status: 'finished',
            config: { totalSteps: 2e6, worlds: 1, slots: 4, seed: 1 },
            createdAt: '2026-11-23T00:00:00Z',
            latest: { globalStep: 2e6, deathsPer1kBeats: 12.4 },
          },
        ],
      })
    })
    vi.stubGlobal('fetch', fetchMock)
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/training', name: 'training', component: TrainingView },
        { path: '/training/:id', name: 'training-run', component: { template: '<div />' } },
      ],
    })
    await router.push('/training')
    const wrapper = mount(TrainingView, { global: { plugins: [router] } })
    await flushPromises()
    expect(wrapper.text()).toContain('ppo-old')
    expect(wrapper.text()).toContain('2.0M / 2.0M steps')
    expect(wrapper.text()).toContain('12 deaths/1k beats')

    await wrapper.find('input[name="millions"]').setValue('2.5')
    await wrapper.find('form').trigger('submit')
    await flushPromises()
    const post = fetchMock.mock.calls.find(([, init]) => init?.method === 'POST')!
    expect(String(post[0])).toBe('/api/training-runs')
    expect(JSON.parse(String(post[1]!.body))).toEqual({
      totalSteps: 2_500_000,
      worlds: 16,
      slots: 64,
      seed: 1,
    })
    expect(router.currentRoute.value.fullPath).toBe('/training/ppo-new')
  })
})
