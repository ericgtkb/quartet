/** Read a design token (CSS custom property) from the root element. */
export function token(name: string, fallback = '#888888'): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return value || fallback
}

export interface StageColors {
  background: string
  floor: string
  grid: string
  voices: [string, string, string, string]
  mote: string
  static: string
  chord: string
}

/** The stage palette for the current theme, from styles/tokens.css. */
export function stageColors(): StageColors {
  return {
    background: token('--color-bg'),
    floor: token('--color-surface-raised'),
    grid: token('--color-border'),
    voices: [
      token('--color-voice-soprano'),
      token('--color-voice-alto'),
      token('--color-voice-tenor'),
      token('--color-voice-bass'),
    ],
    mote: token('--color-mote'),
    static: token('--color-static'),
    chord: token('--color-accent'),
  }
}
