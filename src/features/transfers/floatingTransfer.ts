import type { InterfaceLanguage } from '../../types/backend'
import type { TransferItem } from '../../types/files'
import type { ThemeId } from '../../theme'

export type FloatingTransferMode = 'active' | 'paused' | 'idle'
export type FloatingTransferDirection = 'upload' | 'download' | 'mixed' | null

export interface FloatingTransferSummary {
  readonly mode: FloatingTransferMode
  readonly direction: FloatingTransferDirection
  readonly percent: number | null
  readonly taskCount: number
}

export interface FloatingTransferSnapshot extends FloatingTransferSummary {
  readonly completionVersion: number
  readonly language: InterfaceLanguage
  readonly reduceMotion: boolean
  readonly themeId: ThemeId
  readonly compactEnabled: boolean
  readonly clickToggleEnabled: boolean
  readonly doubleClickOpenEnabled: boolean
  readonly resumablePausedCount: number
}

export function shouldPromptForFloatingWindow(
  signedIn: boolean,
  settingsLoaded: boolean,
  prompted: boolean,
  modalOpen: boolean,
) {
  return signedIn && settingsLoaded && !prompted && !modalOpen
}

const ACTIVE_STATES = new Set(['queued', 'running', 'retrying'])
const UNFINISHED_STATES = new Set([...ACTIVE_STATES, 'paused'])

export function summarizeFloatingTransfers(
  items: readonly TransferItem[],
): FloatingTransferSummary {
  const unfinished = items.filter((item) => UNFINISHED_STATES.has(item.state))
  const active = unfinished.filter((item) => ACTIVE_STATES.has(item.state))
  const mode: FloatingTransferMode = active.length > 0
    ? 'active'
    : unfinished.length > 0 ? 'paused' : 'idle'
  if (mode === 'idle') {
    return { mode, direction: null, percent: null, taskCount: 0 }
  }

  const directions = new Set(unfinished.map((item) => item.direction))
  const direction: FloatingTransferDirection = directions.size > 1
    ? 'mixed'
    : directions.has('upload') ? 'upload' : 'download'
  const allTotalsKnown = unfinished.every(
    (item) => item.totalBytes !== null && item.totalBytes > 0,
  )
  const totalBytes = allTotalsKnown
    ? unfinished.reduce((sum, item) => sum + (item.totalBytes ?? 0), 0)
    : 0
  const bytesTransferred = unfinished.reduce(
    (sum, item) => sum + Math.max(0, item.bytesTransferred),
    0,
  )
  const percent = allTotalsKnown && totalBytes > 0
    ? Math.min(100, Math.max(0, (bytesTransferred / totalBytes) * 100))
    : null

  return {
    mode,
    direction,
    percent,
    taskCount: unfinished.length,
  }
}
