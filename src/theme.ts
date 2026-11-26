export const themes = [
  {
    id: 'koofr',
    label: 'Koofr 绿',
    accent: '#2fa84f',
    accentDark: '#238a3e',
    accentContrast: '#ffffff',
    accentSoft: '#eef9f1',
    accentSoftStrong: '#dcf3e2',
  },
  {
    id: 'ocean',
    label: '海湾蓝',
    accent: '#2b6ee8',
    accentDark: '#1f58c4',
    accentContrast: '#ffffff',
    accentSoft: '#eef4ff',
    accentSoftStrong: '#dde9ff',
  },
  {
    id: 'iris',
    label: '鸢尾紫',
    accent: '#694ed6',
    accentDark: '#553ab8',
    accentContrast: '#ffffff',
    accentSoft: '#f3f0ff',
    accentSoftStrong: '#e6e0ff',
  },
  {
    id: 'coral',
    label: '珊瑚红',
    accent: '#e04f4a',
    accentDark: '#c43c37',
    accentContrast: '#ffffff',
    accentSoft: '#fef1f0',
    accentSoftStrong: '#fde2e0',
  },
  {
    id: 'berry',
    label: '莓果粉',
    accent: '#bc4372',
    accentDark: '#9e325c',
    accentContrast: '#ffffff',
    accentSoft: '#fef0f6',
    accentSoftStrong: '#f8deea',
  },
] as const

export type ThemeId = (typeof themes)[number]['id']

const STORAGE_KEY = 'koofr-ui-preferences'
const STORAGE_VERSION = 1
const defaultThemeId: ThemeId = 'koofr'
const themeIds = new Set<ThemeId>(themes.map((theme) => theme.id))

interface StoredPreferences {
  version: number
  theme: ThemeId
}

export function readStoredTheme(): ThemeId {
  try {
    const storedValue = window.localStorage.getItem(STORAGE_KEY)
    if (!storedValue) return defaultThemeId

    const preferences = JSON.parse(storedValue) as Partial<StoredPreferences>
    return preferences.version === STORAGE_VERSION && preferences.theme && themeIds.has(preferences.theme)
      ? preferences.theme
      : defaultThemeId
  } catch {
    return defaultThemeId
  }
}

export function storeTheme(theme: ThemeId) {
  try {
    const preferences: StoredPreferences = { version: STORAGE_VERSION, theme }
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(preferences))
  } catch {
    // The selected theme still applies for this session when storage is unavailable.
  }
}
