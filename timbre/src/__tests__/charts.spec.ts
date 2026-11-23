import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { formatNumber, nearestIndex, niceTicks } from '@/components/ui/chart'
import QHeatmap from '@/components/ui/QHeatmap.vue'
import QLineChart from '@/components/ui/QLineChart.vue'

describe('chart helpers', () => {
  it('makes round ticks that cover the range', () => {
    expect(niceTicks(0, 97)).toEqual([0, 25, 50, 75, 100])
    expect(niceTicks(0.12, 0.83)).toEqual([0, 0.2, 0.4, 0.6, 0.8, 1])
    const flat = niceTicks(5, 5)
    expect(flat[0]).toBeLessThan(5)
    expect(flat[flat.length - 1]).toBeGreaterThan(5)
  })

  it('formats full under 10,000 and compact above', () => {
    expect(formatNumber(1284)).toBe('1,284')
    expect(formatNumber(253_000)).toBe('253K')
    expect(formatNumber(1.234)).toBe('1.23')
    expect(formatNumber(0.01234)).toBe('0.012')
  })

  it('finds the nearest x', () => {
    expect(nearestIndex([0, 10, 20, 30], 14)).toBe(1)
    expect(nearestIndex([0, 10, 20, 30], 16)).toBe(2)
    expect(nearestIndex([0, 10, 20, 30], 99)).toBe(3)
  })
})

describe('QLineChart', () => {
  const series = [
    { label: 'soprano', color: 'red', values: [1, 2, 3] },
    { label: 'bass', color: 'blue', values: [3, 2, 1] },
  ]

  it('draws one line per series with a legend, and a table view', async () => {
    const wrapper = mount(QLineChart, {
      props: { title: 'Energy', x: [10, 20, 30], series, xLabel: 'beat' },
    })
    expect(wrapper.findAll('.chart__line')).toHaveLength(2)
    expect(wrapper.find('.chart__legend').text()).toContain('soprano')

    await wrapper.find('.chart__toggle').trigger('click')
    const rows = wrapper.findAll('tbody tr')
    expect(rows).toHaveLength(3)
    expect(rows[2]!.text()).toContain('30')
  })

  it('has no legend for one series and shows values on keyboard focus', async () => {
    const wrapper = mount(QLineChart, {
      props: { title: 'Deaths', x: [1, 2, 3], series: [series[0]!], xLabel: 'beat' },
    })
    expect(wrapper.find('.chart__legend').exists()).toBe(false)
    expect(wrapper.find('.chart__end').text()).toBe('3')
    await wrapper.find('svg').trigger('keydown', { key: 'ArrowLeft' })
    expect(wrapper.find('.chart__tooltip').text()).toContain('beat 2')
  })

  it('keeps an all-zero series on a 0-based axis', () => {
    const wrapper = mount(QLineChart, {
      props: {
        title: 'Chords',
        x: [1, 2],
        series: [{ label: 'c', color: 'red', values: [0, 0] }],
        xLabel: 'beat',
      },
    })
    const ticks = wrapper.findAll('.chart__axis text').map((t) => Number(t.text()))
    expect(Math.min(...ticks)).toBe(0)
  })

  it('says so when there is no data', () => {
    const wrapper = mount(QLineChart, { props: { title: 'X', x: [], series: [], xLabel: 'beat' } })
    expect(wrapper.text()).toContain('No data yet.')
  })
})

describe('QHeatmap', () => {
  it('draws row 0 at the bottom and reports counts on focus', async () => {
    const wrapper = mount(QHeatmap, {
      props: { label: 'density', columns: 2, rows: 2, counts: [1, 2, 3, 4] },
    })
    const cells = wrapper.findAll('.heatmap__cell')
    expect(cells.map((c) => c.attributes('aria-label'))).toEqual([
      'cell 0, 1: 3',
      'cell 1, 1: 4',
      'cell 0, 0: 1',
      'cell 1, 0: 2',
    ])
    await cells[1]!.trigger('focus')
    expect(wrapper.find('[role="status"]').text()).toContain('4')
  })
})
