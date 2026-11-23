import { computed, ref } from 'vue'

export type Theme = 'light' | 'dark'

export const THEME_STORAGE_KEY = 'quartet-theme'

function readOverride(): Theme | null {
  try {
    const value = localStorage.getItem(THEME_STORAGE_KEY)
    return value === 'light' || value === 'dark' ? value : null
  } catch {
    return null // storage can be unavailable (private mode, blocked site data)
  }
}

function systemQuery(): MediaQueryList | null {
  return typeof window.matchMedia === 'function'
    ? window.matchMedia('(prefers-color-scheme: dark)')
    : null
}

// Module-level state: one theme for the whole app.
const override = ref<Theme | null>(readOverride())
const systemDark = ref(systemQuery()?.matches ?? false)
systemQuery()?.addEventListener('change', (event) => {
  systemDark.value = event.matches
})

const theme = computed<Theme>(() => override.value ?? (systemDark.value ? 'dark' : 'light'))

/**
 * The active color theme. It follows the OS preference until the user
 * toggles it; the choice is then stored and applied as `data-theme` on <html>,
 * which switches the tokens in styles/tokens.css. index.html applies a stored
 * choice before first paint to avoid a flash of the wrong theme.
 */
export function useTheme() {
  function setTheme(next: Theme) {
    override.value = next
    document.documentElement.dataset.theme = next
    try {
      localStorage.setItem(THEME_STORAGE_KEY, next)
    } catch {
      // The choice still applies for this session.
    }
  }

  function toggle() {
    setTheme(theme.value === 'dark' ? 'light' : 'dark')
  }

  return { theme, setTheme, toggle }
}
