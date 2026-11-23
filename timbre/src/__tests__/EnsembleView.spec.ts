import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import EnsembleView from '@/views/EnsembleView.vue'
import type { EnsembleResponse } from '@/api/types'

afterEach(() => vi.unstubAllGlobals())

const ensemble: EnsembleResponse = {
  members: [
    { name: 'rhythm', status: 'ok', version: '0.1.0', runtime: 'Go 1.27.4', details: {} },
    {
      name: 'harmony',
      status: 'ok',
      version: '0.1.0',
      runtime: 'Rust 1.100.0',
      details: { observationVersion: '0' },
    },
    { name: 'melody', status: 'unavailable', details: {}, error: 'connection refused' },
  ],
}

describe('EnsembleView', () => {
  it('renders timbre plus every member from rhythm', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(ensemble))),
    )
    const wrapper = mount(EnsembleView)
    await flushPromises()

    const names = wrapper.findAll('h3').map((h) => h.text())
    expect(names).toEqual(['timbre', 'rhythm', 'harmony', 'melody'])
    expect(wrapper.text()).toContain('observationVersion')
    expect(wrapper.text()).toContain('connection refused')
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
  })

  it('shows an alert and still renders timbre when rhythm is unreachable', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>().mockRejectedValue(new TypeError('Failed to fetch')),
    )
    const wrapper = mount(EnsembleView)
    await flushPromises()

    expect(wrapper.find('[role="alert"]').text()).toContain('rhythm is unreachable')
    expect(wrapper.findAll('h3').map((h) => h.text())).toEqual(['timbre'])
  })
})
