import type {
  FileToolsEntry,
  FileToolsMount,
  FileToolsResults,
} from '../../types/fileTools'

export type FileToolId = 'usage' | 'empty' | 'duplicates'
export type FileToolsWork = 'scan' | 'verify' | 'remove' | 'browse'

export interface FileToolsRemoval {
  readonly target: FileToolsEntry
  readonly keep?: FileToolsEntry
}

export interface FileToolsSummary {
  readonly totalBytes: number
  readonly totalFiles: number
  readonly emptyCount: number
  readonly removableEmptyCount: number
  readonly duplicateGroupCount: number
  readonly duplicateFileCount: number
  readonly duplicateOverheadBytes: number
  readonly removableReclaimableBytes: number
  readonly candidateGroupCount: number
  readonly candidateFileCount: number
  readonly candidateEstimatedBytes: number
  readonly hasIncompleteRoot: boolean
}

export function entryKey(entry: FileToolsEntry): string {
  return `${entry.id.scope}/${entry.id.entry}`
}

export function entryBasename(path: string): string {
  const normalized = path.replace(/\/+$/, '')
  if (!normalized) return '/'
  const idx = normalized.lastIndexOf('/')
  return idx >= 0 ? normalized.slice(idx + 1) || '/' : normalized
}

export function entryParentPath(path: string): string {
  const normalized = path.replace(/\/+$/, '')
  if (!normalized || normalized === '/') return '/'
  const idx = normalized.lastIndexOf('/')
  if (idx <= 0) return '/'
  return normalized.slice(0, idx)
}

export function cleanableScopesForGroup(entries: readonly FileToolsEntry[]): Set<number> {
  const counts = new Map<number, number>()
  for (const entry of entries) {
    if (!entry.removable) continue
    counts.set(entry.id.scope, (counts.get(entry.id.scope) ?? 0) + 1)
  }
  const scopes = new Set<number>()
  for (const [scope, count] of counts) {
    if (count >= 2) scopes.add(scope)
  }
  return scopes
}

export function summarizeResults(results: FileToolsResults): FileToolsSummary {
  let totalBytes = 0
  let totalFiles = 0
  let hasIncompleteRoot = false
  for (const root of results.roots) {
    totalBytes += root.size
    totalFiles += root.files
    if (!root.complete) hasIncompleteRoot = true
  }

  let removableEmptyCount = 0
  for (const folder of results.emptyFolders) {
    if (folder.removable) removableEmptyCount += 1
  }

  let duplicateFileCount = 0
  let duplicateOverheadBytes = 0
  let removableReclaimableBytes = 0
  for (const group of results.duplicates) {
    if (group.length === 0) continue
    const size = group[0].size
    duplicateFileCount += group.length
    duplicateOverheadBytes += size * Math.max(0, group.length - 1)
    const perScope = new Map<number, number>()
    for (const entry of group) {
      if (!entry.removable) continue
      perScope.set(entry.id.scope, (perScope.get(entry.id.scope) ?? 0) + 1)
    }
    for (const count of perScope.values()) {
      if (count >= 2) {
        removableReclaimableBytes += size * (count - 1)
      }
    }
  }

  let candidateFileCount = 0
  let candidateEstimatedBytes = 0
  for (const candidate of results.candidates) {
    candidateFileCount += candidate.entries.length
    candidateEstimatedBytes += candidate.estimatedBytes
  }

  return {
    totalBytes,
    totalFiles,
    emptyCount: results.emptyFolders.length,
    removableEmptyCount,
    duplicateGroupCount: results.duplicates.length,
    duplicateFileCount,
    duplicateOverheadBytes,
    removableReclaimableBytes,
    candidateGroupCount: results.candidates.length,
    candidateFileCount,
    candidateEstimatedBytes,
    hasIncompleteRoot,
  }
}

export function describeScannedScopes(results: FileToolsResults): string {
  const names = results.roots.map((root) => {
    const mount = results.mounts[root.id.scope]
    const name = mount?.name || '存储位置'
    return root.path === '/' ? name : `${name} (${root.path})`
  })
  if (names.length === 0) return ''
  if (names.length <= 2) return names.join('、')
  return `${names.slice(0, 2).join('、')} 等 ${names.length} 个位置`
}

export function formatSharePercent(size: number, total: number): string {
  if (total <= 0 || size <= 0) return '0%'
  const pct = (size / total) * 100
  if (pct < 0.1) return '<0.1%'
  if (pct >= 10) return `${pct.toFixed(0)}%`
  return `${pct.toFixed(1)}%`
}

export function onlineMountIds(mounts: readonly FileToolsMount[]): string[] {
  return mounts.filter((mount) => mount.online).map((mount) => mount.id)
}
