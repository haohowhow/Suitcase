import type { InterfaceLanguage } from '../../types/backend'
import type { TransferItem } from '../../types/files'
import type { SyncPairSummary } from '../../types/backend'
import type { ThemeId } from '../../theme'

export type FloatingTransferMode = 'active' | 'attention' | 'offline' | 'paused' | 'idle'
export type FloatingTransferDirection = 'upload' | 'download' | 'mixed' | null
export type FloatingActivityKind = 'transfer' | 'sync' | 'mixed' | 'none'

export interface FloatingTransferSummary {
  readonly mode: FloatingTransferMode
  readonly direction: FloatingTransferDirection
  readonly percent: number | null
  readonly taskCount: number
  readonly activityKind: FloatingActivityKind
  readonly syncPairCount: number
  readonly syncIssueCount: number
  readonly uploadBytesPerSecond: number
  readonly downloadBytesPerSecond: number
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

function currentTransferBytesPerSecond(item: TransferItem) {
  const samples = item.speedSamples
  const current = samples.at(-1)
  const previous = samples.at(-2)
  if (!current || !previous || current.recordedAt <= previous.recordedAt) return 0
  return Math.max(
    0,
    (current.bytesTransferred - previous.bytesTransferred)
      / ((current.recordedAt - previous.recordedAt) / 1000),
  )
}

export function summarizeFloatingTransfers(
  items: readonly TransferItem[],
  syncPairs: readonly SyncPairSummary[] = [],
): FloatingTransferSummary {
  const unfinished = items.filter((item) => UNFINISHED_STATES.has(item.state))
  const active = unfinished.filter((item) => ACTIVE_STATES.has(item.state))
  const activeSync = syncPairs.filter((pair) => pair.status === 'initializing' || pair.status === 'scanning' || pair.status === 'syncing')
  const attentionSync = syncPairs.some((pair) => pair.status === 'attention' || pair.issueCount > 0)
  const offlineSync = syncPairs.some((pair) => pair.status === 'offline')
  const pausedSync = syncPairs.filter((pair) => pair.paused || pair.status === 'paused')
  const hasActive = active.length > 0 || activeSync.length > 0
  const mode: FloatingTransferMode = hasActive
    ? 'active'
    : attentionSync ? 'attention'
      : offlineSync ? 'offline'
        : unfinished.length > 0 || pausedSync.length > 0 ? 'paused' : 'idle'
  const activityKind: FloatingActivityKind = unfinished.length > 0 && (activeSync.length > 0 || pausedSync.length > 0)
    ? 'mixed'
    : unfinished.length > 0 ? 'transfer'
      : activeSync.length > 0 || pausedSync.length > 0 ? 'sync' : 'none'
  const syncIssueCount = syncPairs.reduce((sum, pair) => sum + pair.issueCount, 0)
  const transferUploadBytesPerSecond = active
    .filter((item) => item.direction === 'upload')
    .reduce((sum, item) => sum + currentTransferBytesPerSecond(item), 0)
  const transferDownloadBytesPerSecond = active
    .filter((item) => item.direction === 'download')
    .reduce((sum, item) => sum + currentTransferBytesPerSecond(item), 0)
  const uploadBytesPerSecond = transferUploadBytesPerSecond
    + activeSync.reduce((sum, pair) => sum + pair.upload.bytesPerSecond, 0)
  const downloadBytesPerSecond = transferDownloadBytesPerSecond
    + activeSync.reduce((sum, pair) => sum + pair.download.bytesPerSecond, 0)
  const base = { activityKind, syncPairCount: syncPairs.length, syncIssueCount, uploadBytesPerSecond, downloadBytesPerSecond }
  if (mode === 'idle') {
    return { ...base, mode, direction: null, percent: null, taskCount: 0 }
  }

  const directions = new Set(unfinished.map((item) => item.direction))
  if (activeSync.some((pair) => pair.upload.pendingItems > 0)) directions.add('upload')
  if (activeSync.some((pair) => pair.download.pendingItems > 0)) directions.add('download')
  const direction: FloatingTransferDirection = directions.size > 1
    ? 'mixed'
    : directions.has('upload') ? 'upload' : directions.has('download') ? 'download' : null
  const allTotalsKnown = unfinished.every(
    (item) => item.totalBytes !== null && item.totalBytes > 0,
  )
  const syncTotalsKnown = activeSync.every((pair) => pair.upload.totalBytes + pair.download.totalBytes > 0)
  const syncTotalBytes = activeSync.reduce((sum, pair) => sum + pair.upload.totalBytes + pair.download.totalBytes, 0)
  const totalBytes = allTotalsKnown && syncTotalsKnown
    ? unfinished.reduce((sum, item) => sum + (item.totalBytes ?? 0), 0) + syncTotalBytes
    : 0
  const bytesTransferred = unfinished.reduce(
    (sum, item) => sum + Math.max(0, item.bytesTransferred),
    0,
  ) + activeSync.reduce((sum, pair) => sum + pair.upload.completedBytes + pair.download.completedBytes, 0)
  const percent = allTotalsKnown && totalBytes > 0
    ? Math.min(100, Math.max(0, (bytesTransferred / totalBytes) * 100))
    : null

  return {
    mode,
    direction,
    percent,
    taskCount: unfinished.length + activeSync.length + pausedSync.length,
    ...base,
  }
}
