import { beforeEach, describe, expect, it } from 'vitest'
import { THEME_STORAGE_KEY, useTheme } from '@/composables/useTheme'

describe('useTheme', () => {
  beforeEach(() => {
    localStorage.clear()
    delete document.documentElement.dataset.theme
  })

  it('toggles, applies data-theme, and persists the choice', () => {
    const { theme, toggle } = useTheme()
    const before = theme.value
    const after = before === 'dark' ? 'light' : 'dark'

    toggle()

    expect(theme.value).toBe(after)
    expect(document.documentElement.dataset.theme).toBe(after)
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe(after)
  })

  it('setTheme sets an explicit theme', () => {
    const { theme, setTheme } = useTheme()
    setTheme('dark')
    expect(theme.value).toBe('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
  })
})
