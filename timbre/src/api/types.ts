// Public API types. Hand-written mirror of rhythm/internal/api/types.go;
// change both together (see AGENTS.md).

export type MemberStatus = 'ok' | 'unavailable'

export interface Member {
  name: string
  status: MemberStatus
  version?: string
  runtime?: string
  details: Record<string, string>
  error?: string
}

/** GET /api/ensemble */
export interface EnsembleResponse {
  members: Member[]
}

/** GET /api/healthz */
export interface HealthResponse {
  status: 'ok' | 'degraded'
  checks: Record<string, MemberStatus>
}

/** Body of every error response. */
export interface ErrorResponse {
  error: string
}

export type PolicyKind = 'builtin' | 'checkpoint'

/** A policy a performance can use. GET /api/policies */
export interface Policy {
  name: string
  kind: PolicyKind
  description: string
  /** Checkpoints only: agent-steps trained. */
  globalStep?: number
  /** Checkpoints only: the training run's status. */
  trainingStatus?: TrainingStatus
}

export interface PoliciesResponse {
  policies: Policy[]
}

/** POST /api/performances body. Omitted fields take server defaults. */
export interface PerformanceConfig {
  policy: string
  slots?: number
  seed?: number
  beatsPerSecond?: number
  /** Fill vacated slots with offspring (default true). */
  evolve?: boolean
}

export type PerformanceStatus = 'playing' | 'failed'

/** A performance's description (rhythm performance.Info). */
export interface Performance {
  id: string
  policy: string
  policyKind: PolicyKind
  slots: number
  seed: number
  beatsPerSecond: number
  evolve: boolean
  beat: number
  status: PerformanceStatus
  error?: string
  subscribers: number
  createdAt: string
}

export interface PerformancesResponse {
  performances: Performance[]
}

/** Voice index in frames: 0 soprano, 1 alto, 2 tenor, 3 bass. */
export type VoiceIndex = 0 | 1 | 2 | 3

export const VOICES = ['soprano', 'alto', 'tenor', 'bass'] as const

/**
 * One beat of a performance (SSE "frame" event; rhythm performance.Frame).
 * Per-quartetling fields are parallel arrays indexed by slot.
 */
export interface Frame {
  beat: number
  width: number
  height: number
  quartetlings: {
    id: number[]
    voice: VoiceIndex[]
    x: number[]
    y: number[]
    heading: number[]
    energy: number[]
    breath: number[]
    /** 0 or 1 */
    singing: number[]
    /** 0 or 1 */
    inChord: number[]
    age: number[]
    generation: number[]
  }
  motes: { x: number[]; y: number[] }
  statics: { x: number[]; y: number[]; radius: number[] }
  stats: { motesEaten: number; deaths: number; chordMembers: number; births: number }
}

/** SSE "end" event data. */
export interface StreamEnd {
  status: PerformanceStatus
}

export interface TraitStats {
  mean: number
  std: number
}

/** Aggregates over one voice (rhythm api.VoiceCensus). */
export interface VoiceCensus {
  voice: VoiceIndex
  population: number
  meanEnergy: number
  meanAge: number
  tempo: TraitStats
  pitch: TraitStats
  dynamics: TraitStats
  singing: number
  inChord: number
  meanGeneration: number
  maxGeneration: number
}

/** Equal-width bins over [min, max]; counts[voice][bin]. */
export interface Histogram {
  min: number
  max: number
  counts: number[][]
}

/** GET /api/performances/{id}/census */
export interface Census {
  beat: number
  slots: number
  voices: VoiceCensus[]
  histograms: { tempo: Histogram; pitch: Histogram; dynamics: Histogram; energy: Histogram }
  /** counts[row * columns + column], row 0 at y = 0. */
  density: { columns: number; rows: number; counts: number[]; singing: number[] }
  /** One entry per window of `beats` beats, oldest first; per-voice series are [voice][sample]. */
  history: {
    beat: number[]
    beats: number[]
    deaths: number[]
    motesEaten: number[]
    chordMembers: number[]
    singing: number[]
    births: number[]
    maxGeneration: number[]
    lineages: number[]
    meanEnergy: number[][]
    meanTempo: number[][]
    meanPitch: number[][]
    meanDynamics: number[][]
    meanGeneration: number[][]
  }
  /** Distinct founder lineages with a living member. */
  lineages: number
}

export type TrainingStatus = 'running' | 'finished' | 'stopped' | 'failed' | 'unknown'

/** POST /api/training-runs body; omitted or zero fields take melody's defaults. */
export interface TrainingConfig {
  totalSteps: number
  worlds: number
  slots: number
  seed: number
}

/** One PPO update (SSE "metrics" event; rhythm api.TrainingMetrics). */
export interface TrainingMetrics {
  update: number
  globalStep: number
  elapsedSeconds: number
  stepsPerSecond: number
  episodeReturn: number
  /** Mean lifespan, in beats, of the lives that ended in the update. */
  episodeLength: number
  rewardPerStep: number
  chordRate: number
  deathsPer1kBeats: number
  motesPer1kBeats: number
  meanEnergy: number
  policyLoss: number
  valueLoss: number
  entropy: number
  approxKl: number
  clipFraction: number
  explainedVariance: number
  learningRate: number
}

/** A training run (rhythm api.TrainingRun). */
export interface TrainingRun {
  id: string
  status: TrainingStatus
  config: TrainingConfig
  createdAt: string
  error?: string
  latest?: TrainingMetrics
}

export interface TrainingRunsResponse {
  runs: TrainingRun[]
}

/** SSE "end" event data of a training stream. */
export interface TrainingStreamEnd {
  status: TrainingStatus
}
