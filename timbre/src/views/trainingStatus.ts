import type { TrainingStatus } from '@/api/types'

/** Badge tone for a training status. */
export function statusTone(status: TrainingStatus | null | undefined): 'ok' | 'error' | 'neutral' {
  if (status === 'running' || status === 'finished') return 'ok'
  if (status === 'failed') return 'error'
  return 'neutral'
}
