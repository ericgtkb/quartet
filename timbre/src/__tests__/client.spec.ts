import { afterEach, describe, expect, it, vi } from 'vitest'
import { api, ApiError } from '@/api/client'

afterEach(() => vi.unstubAllGlobals())

describe('api client', () => {
  it('returns parsed JSON on success', async () => {
    const body = { members: [] }
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(body)))
    vi.stubGlobal('fetch', fetchMock)

    await expect(api.ensemble()).resolves.toEqual(body)
    expect(fetchMock).toHaveBeenCalledWith('/api/ensemble', expect.anything())
  })

  it('uses the error field of a JSON error body', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(new Response(JSON.stringify({ error: 'not found' }), { status: 404 })),
    )
    await expect(api.ensemble()).rejects.toMatchObject({ status: 404, message: 'not found' })
  })

  it('reports a network failure as ApiError with no status', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>().mockRejectedValue(new TypeError('Failed to fetch')),
    )
    const error = await api.ensemble().catch((e: unknown) => e)
    expect(error).toBeInstanceOf(ApiError)
    expect(error).toMatchObject({ status: null })
  })

  it('posts training configs and stops runs by id', async () => {
    const fetchMock = vi.fn<typeof fetch>(async () => new Response('{}'))
    vi.stubGlobal('fetch', fetchMock)
    await api.startTraining({ totalSteps: 1000 })
    await api.stopTraining('ppo 1')
    const [startUrl, start] = fetchMock.mock.calls[0]!
    const [stopUrl, stop] = fetchMock.mock.calls[1]!
    expect([startUrl, start!.method, start!.body]).toEqual([
      '/api/training-runs',
      'POST',
      '{"totalSteps":1000}',
    ])
    expect([stopUrl, stop!.method]).toEqual(['/api/training-runs/ppo%201/stop', 'POST'])
    expect(api.trainingStreamUrl('a/b')).toBe('/api/training-runs/a%2Fb/stream')
  })
})
