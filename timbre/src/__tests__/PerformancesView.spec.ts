import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createMemoryHistory, createRouter } from 'vue-router'
import PerformancesView from '@/views/PerformancesView.vue'

afterEach(() => vi.unstubAllGlobals())

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status })

describe('PerformancesView', () => {
  it('lists policies and performances, and starts a performance', async () => {
    const fetchMock = vi.fn<typeof fetch>(async (input, init) => {
      const url = String(input)
      if (url === '/api/policies')
        return json({
          policies: [
            { name: 'choir', kind: 'builtin', description: 'Sings.' },
            { name: 'ppo-1', kind: 'checkpoint', description: 'PPO.', trainingStatus: 'running' },
          ],
        })
      if (url === '/api/performances' && init?.method === 'POST')
        return json({ id: 'p7', policy: 'choir' }, 201)
      return json({ performances: [] })
    })
    vi.stubGlobal('fetch', fetchMock)
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/', name: 'performances', component: PerformancesView },
        { path: '/performances/:id', name: 'performance', component: { template: '<div />' } },
      ],
    })
    await router.push('/')
    const wrapper = mount(PerformancesView, { global: { plugins: [router] } })
    await flushPromises()

    expect(wrapper.text()).toContain('Nothing playing yet.')
    expect(wrapper.find('select').text()).toContain('choir')
    const groups = wrapper.findAll('optgroup')
    expect(groups.map((g) => g.attributes('label'))).toEqual(['Builtin', 'Trained'])
    expect(groups[1]!.text()).toContain('ppo-1 (training)')

    await wrapper.find('input[name="seed"]').setValue('42')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    const post = fetchMock.mock.calls.find(([, init]) => init?.method === 'POST')!
    expect(JSON.parse(String(post[1]!.body))).toEqual({
      policy: 'choir',
      slots: 64,
      beatsPerSecond: 15,
      evolve: true,
      seed: 42,
    })
    expect(router.currentRoute.value.fullPath).toBe('/performances/p7')
  })
})
