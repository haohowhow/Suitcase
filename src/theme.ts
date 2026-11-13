export const themes = [
  {
    id: 'koofr',
    label: 'Koofr 绿',
    accent: '#66c451',
    accentDark: '#50a93f',
    accentContrast: '#142a17',
    accentSoft: '#f0faed',
    accentSoftStrong: '#e3f5de',
  },
  {
    id: 'ocean',
    label: '海湾蓝',
    accent: '#3f7fea',
    accentDark: '#2d68cf',
    accentContrast: '#ffffff',
    accentSoft: '#eef5ff',
    accentSoftStrong: '#dfeaff',
  },
  {
    id: 'iris',
    label: '鸢尾紫',
    accent: '#7659d9',
    accentDark: '#6243c4',
    accentContrast: '#ffffff',
    accentSoft: '#f3f0ff',
    accentSoftStrong: '#e8e1ff',
  },
  {
    id: 'coral',
    label: '珊瑚红',
    accent: '#f05d58',
    accentDark: '#d94a45',
    accentContrast: '#ffffff',
    accentSoft: '#fff1f0',
    accentSoftStrong: '#ffe3e1',
  },
  {
    id: 'berry',
    label: '莓果粉',
    accent: '#c24f7d',
    accentDark: '#a93a68',
    accentContrast: '#ffffff',
    accentSoft: '#fff0f6',
    accentSoftStrong: '#f9dfeb',
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
