import type {
  Census,
  EnsembleResponse,
  ErrorResponse,
  Performance,
  PerformanceConfig,
  PerformancesResponse,
  PoliciesResponse,
  TrainingConfig,
  TrainingRun,
  TrainingRunsResponse,
} from './types'

/** A non-2xx response from rhythm, or a failure to reach it. */
export class ApiError extends Error {
  constructor(
    readonly status: number | null,
    message: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

async function request<T>(
  method: string,
  path: string,
  options: { body?: unknown; signal?: AbortSignal } = {},
): Promise<T> {
  const headers: Record<string, string> = { Accept: 'application/json' }
  if (options.body !== undefined) headers['Content-Type'] = 'application/json'
  let response: Response
  try {
    response = await fetch(`/api${path}`, {
      method,
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      signal: options.signal,
    })
  } catch (cause) {
    if (cause instanceof DOMException && cause.name === 'AbortError') throw cause
    throw new ApiError(null, `cannot reach rhythm: ${String(cause)}`)
  }
  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`.trim()
    try {
      const body = (await response.json()) as Partial<ErrorResponse>
      if (typeof body.error === 'string') message = body.error
    } catch {
      // Not JSON (e.g. the dev proxy's 502 page); keep the status line.
    }
    throw new ApiError(response.status, message)
  }
  if (response.status === 204) return undefined as T
  return (await response.json()) as T
}

export const api = {
  ensemble: (signal?: AbortSignal) => request<EnsembleResponse>('GET', '/ensemble', { signal }),
  policies: (signal?: AbortSignal) => request<PoliciesResponse>('GET', '/policies', { signal }),
  performances: (signal?: AbortSignal) =>
    request<PerformancesResponse>('GET', '/performances', { signal }),
  performance: (id: string, signal?: AbortSignal) =>
    request<Performance>('GET', `/performances/${encodeURIComponent(id)}`, { signal }),
  createPerformance: (config: PerformanceConfig) =>
    request<Performance>('POST', '/performances', { body: config }),
  deletePerformance: (id: string) =>
    request<void>('DELETE', `/performances/${encodeURIComponent(id)}`),
  /** URL of the performance's Server-Sent Events stream. */
  streamUrl: (id: string) => `/api/performances/${encodeURIComponent(id)}/stream`,
  census: (id: string, signal?: AbortSignal) =>
    request<Census>('GET', `/performances/${encodeURIComponent(id)}/census`, { signal }),
  trainingRuns: (signal?: AbortSignal) =>
    request<TrainingRunsResponse>('GET', '/training-runs', { signal }),
  trainingRun: (id: string, signal?: AbortSignal) =>
    request<TrainingRun>('GET', `/training-runs/${encodeURIComponent(id)}`, { signal }),
  startTraining: (config: Partial<TrainingConfig>) =>
    request<TrainingRun>('POST', '/training-runs', { body: config }),
  stopTraining: (id: string) =>
    request<TrainingRun>('POST', `/training-runs/${encodeURIComponent(id)}/stop`),
  /** URL of the training run's Server-Sent Events stream of metrics. */
  trainingStreamUrl: (id: string) => `/api/training-runs/${encodeURIComponent(id)}/stream`,
}
