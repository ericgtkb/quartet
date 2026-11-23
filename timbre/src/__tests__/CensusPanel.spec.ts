import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import type { Census } from '@/api/types'
import CensusPanel from '@/components/CensusPanel.vue'

afterEach(() => vi.unstubAllGlobals())

const hist = {
  min: 0,
  max: 2,
  counts: [
    [1, 0],
    [0, 1],
    [1, 1],
    [0, 0],
  ],
}
const voice = (v: 0 | 1 | 2 | 3) => ({
  voice: v,
  population: 16,
  meanEnergy: 1.234,
  meanAge: 99.6,
  tempo: { mean: 1.5, std: 0.1 },
  pitch: { mean: 15, std: 1 },
  dynamics: { mean: 1, std: 0.05 },
  singing: 2,
  inChord: 1,
  meanGeneration: 0,
  maxGeneration: 0,
})
const perVoice = [
  [1, 1.1],
  [1, 1.1],
  [1, 1.1],
  [1, 1.1],
]
const census: Census = {
  beat: 40,
  slots: 64,
  voices: [voice(0), voice(1), voice(2), voice(3)],
  histograms: { tempo: hist, pitch: hist, dynamics: hist, energy: hist },
  density: { columns: 2, rows: 2, counts: [1, 2, 3, 4], singing: [0, 0, 1, 0] },
  history: {
    beat: [10, 20],
    beats: [10, 10],
    deaths: [1, 0],
    motesEaten: [3, 4],
    chordMembers: [20, 10],
    singing: [5, 6],
    births: [0, 3],
    maxGeneration: [0, 2],
    lineages: [64, 61],
    meanEnergy: perVoice,
    meanTempo: perVoice,
    meanPitch: perVoice,
    meanDynamics: perVoice,
    meanGeneration: perVoice,
  },
  lineages: 61,
}

describe('CensusPanel', () => {
  it('fetches the census and shows voices, history, histograms, and density', async () => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(census)))
    vi.stubGlobal('fetch', fetchMock)
    const wrapper = mount(CensusPanel, { props: { id: 'p3', intervalMs: 60_000 } })
    await flushPromises()

    expect(fetchMock).toHaveBeenCalledWith('/api/performances/p3/census', expect.anything())
    const rows = wrapper.findAll('tbody tr')
    expect(rows[0]!.text()).toContain('soprano')
    expect(rows[0]!.text()).toContain('1.23')
    expect(rows[0]!.text()).toContain('1.50 ± 0.10')
    // Chord members per beat: 20 / 10 beats = 2 at the last-but-one sample, 1 at the last.
    const charts = wrapper.findAll('.chart')
    expect(charts.map((c) => c.find('.chart__title').text())).toContain(
      'Quartetlings in a chord, per beat',
    )
    expect(wrapper.findAll('.columns')).toHaveLength(16)
    expect(wrapper.text()).toContain('Evolution')
    expect(wrapper.text()).toContain('61 founder lineages alive')
    expect(charts.map((c) => c.find('.chart__title').text())).toContain('Mean tempo by voice')
    expect(wrapper.findAll('.heatmap__cell')).toHaveLength(4)
    wrapper.unmount()
  })
})
