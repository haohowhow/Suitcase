import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { commandErrorMessage, isCommandErrorCode, koofr } from '../services/koofr'
import type {
  FileToolsEntry,
  FileToolsMount,
  FileToolsProgress,
  FileToolsResults,
  FileToolsScope,
} from '../types/fileTools'
import {
  onlineMountIds,
  summarizeResults,
  type FileToolId,
  type FileToolsRemoval,
  type FileToolsSummary,
  type FileToolsWork,
} from '../features/file-tools/fileToolsHelpers'

export type FileToolsView = 'hub' | FileToolId

export interface ActiveScanScopeDisplay {
  readonly mountId: string
  readonly name: string
  readonly path: string
}

export interface FileToolsController {
  readonly view: FileToolsView
  readonly preferredTool: FileToolId
  readonly mounts: readonly FileToolsMount[]
  readonly mountsLoading: boolean
  readonly sharedUnavailable: boolean
  readonly selectedMounts: readonly string[]
  readonly onlyCurrentFolder: boolean
  readonly results: FileToolsResults | null
  readonly summary: FileToolsSummary | null
  readonly breadcrumb: readonly FileToolsEntry[]
  readonly children: readonly FileToolsEntry[]
  readonly work: FileToolsWork | null
  readonly scanRefresh: boolean
  readonly activeScanScopes: readonly ActiveScanScopeDisplay[]
  readonly verifyingSize: number | null
  readonly progress: FileToolsProgress
  readonly notice: string
  readonly error: string
  readonly setView: (view: FileToolsView) => void
  readonly setPreferredTool: (tool: FileToolId) => void
  readonly ensureMountsLoaded: () => void
  readonly refreshMounts: () => void
  readonly toggleMount: (mountId: string, checked: boolean) => void
  readonly selectAllOnline: () => void
  readonly selectPrimaryOnly: () => void
  readonly setOnlyCurrentFolder: (checked: boolean) => void
  readonly scan: (options: {
    readonly refresh: boolean
    readonly targetTool?: FileToolId
    readonly currentMountId: string
    readonly currentPath: string
  }) => Promise<void>
  readonly cancelScan: () => void
  readonly browse: (next: readonly FileToolsEntry[]) => Promise<void>
  readonly verify: (size: number) => Promise<void>
  readonly cancelVerification: () => void
  readonly remove: (removal: FileToolsRemoval) => Promise<void>
}

const INITIAL_PROGRESS: FileToolsProgress = { folders: 0, bytes: 0 }

interface UseFileToolsManagerOptions {
  readonly authState: string
  readonly onRemoteChange: () => void
  readonly onBackgroundScanComplete?: () => void
}

export function useFileToolsManager({
  authState,
  onRemoteChange,
  onBackgroundScanComplete,
}: UseFileToolsManagerOptions): FileToolsController {
  const [view, setView] = useState<FileToolsView>('hub')
  const [preferredTool, setPreferredTool] = useState<FileToolId>('usage')
  const [mounts, setMounts] = useState<readonly FileToolsMount[]>([])
  const [mountsLoading, setMountsLoading] = useState(false)
  const [sharedUnavailable, setSharedUnavailable] = useState(false)
  const [selectedMounts, setSelectedMounts] = useState<readonly string[]>([])
  const [onlyCurrentFolder, setOnlyCurrentFolder] = useState(false)

  const [results, setResults] = useState<FileToolsResults | null>(null)
  const [breadcrumb, setBreadcrumb] = useState<readonly FileToolsEntry[]>([])
  const [children, setChildren] = useState<readonly FileToolsEntry[]>([])

  const [work, setWork] = useState<FileToolsWork | null>(null)
  const [scanRefresh, setScanRefresh] = useState(false)
  const [activeScanScopes, setActiveScanScopes] = useState<readonly ActiveScanScopeDisplay[]>([])
  const [verifyingSize, setVerifyingSize] = useState<number | null>(null)
  const [progress, setProgress] = useState<FileToolsProgress>(INITIAL_PROGRESS)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')

  const runRef = useRef<string | null>(null)
  const sequenceRef = useRef(0)
  const mountRequestRef = useRef(0)
  const mountsLoadedRef = useRef(false)
  const hasInitializedSelectionRef = useRef(false)
  const onRemoteChangeRef = useRef(onRemoteChange)
  const onBackgroundScanCompleteRef = useRef(onBackgroundScanComplete)

  useEffect(() => {
    onRemoteChangeRef.current = onRemoteChange
  }, [onRemoteChange])

  useEffect(() => {
    onBackgroundScanCompleteRef.current = onBackgroundScanComplete
  }, [onBackgroundScanComplete])

  const loadMounts = useCallback((clearError: boolean) => {
    const request = ++mountRequestRef.current
    mountsLoadedRef.current = true
    setMountsLoading(true)
    if (clearError) setError('')
    void koofr.fileToolsMounts().then((loaded) => {
      if (mountRequestRef.current !== request) return
      setMounts(loaded.mounts)
      setSharedUnavailable(loaded.sharedUnavailable)
      const onlineIds = onlineMountIds(loaded.mounts)
      const onlineSet = new Set(onlineIds)
      if (!hasInitializedSelectionRef.current) {
        hasInitializedSelectionRef.current = true
        setSelectedMounts(onlineIds)
      } else {
        setSelectedMounts((current) => {
          const preserved = current.filter((id) => onlineSet.has(id))
          return preserved.length > 0 ? preserved : onlineIds
        })
      }
    }).catch((failure: unknown) => {
      if (mountRequestRef.current === request) {
        mountsLoadedRef.current = false
        setError(commandErrorMessage(failure, '无法读取存储位置。'))
      }
    }).finally(() => {
      if (mountRequestRef.current === request) {
        setMountsLoading(false)
      }
    })
  }, [])

  const ensureMountsLoaded = useCallback(() => {
    if (!mountsLoadedRef.current) {
      loadMounts(false)
    }
  }, [loadMounts])

  const refreshMounts = useCallback(() => {
    loadMounts(true)
  }, [loadMounts])

  // Reset and close any active scan only when signing out or unmounting the app.
  useEffect(() => {
    if (authState === 'signedIn') return
    sequenceRef.current += 1
    mountRequestRef.current += 1
    mountsLoadedRef.current = false
    hasInitializedSelectionRef.current = false
    if (runRef.current) {
      void koofr.closeFileTools(runRef.current).catch(() => {})
      runRef.current = null
    }
    setView('hub')
    setPreferredTool('usage')
    setMounts([])
    setMountsLoading(false)
    setSharedUnavailable(false)
    setSelectedMounts([])
    setOnlyCurrentFolder(false)
    setResults(null)
    setBreadcrumb([])
    setChildren([])
    setWork(null)
    setScanRefresh(false)
    setActiveScanScopes([])
    setVerifyingSize(null)
    setProgress(INITIAL_PROGRESS)
    setNotice('')
    setError('')
  }, [authState])

  useEffect(() => () => {
    sequenceRef.current += 1
    mountRequestRef.current += 1
    if (runRef.current) {
      void koofr.closeFileTools(runRef.current).catch(() => {})
      runRef.current = null
    }
  }, [])

  const summary = useMemo(() => (results ? summarizeResults(results) : null), [results])

  const toggleMount = useCallback((mountId: string, checked: boolean) => {
    setSelectedMounts((ids) => (checked ? [...ids, mountId] : ids.filter((id) => id !== mountId)))
  }, [])

  const selectAllOnline = useCallback(() => {
    setSelectedMounts(onlineMountIds(mounts))
  }, [mounts])

  const selectPrimaryOnly = useCallback(() => {
    setSelectedMounts(mounts.filter((mount) => mount.online && mount.primary).map((mount) => mount.id))
  }, [mounts])

  const scan = useCallback(async ({
    refresh,
    targetTool,
    currentMountId,
    currentPath,
  }: {
    readonly refresh: boolean
    readonly targetTool?: FileToolId
    readonly currentMountId: string
    readonly currentPath: string
  }) => {
    const request = ++sequenceRef.current
    const destinationTool = targetTool ?? (view === 'hub' ? preferredTool : view)
    const canScopeToFolder = currentPath !== '/'
      && selectedMounts.length === 1
      && selectedMounts[0] === currentMountId
    const scopePath = canScopeToFolder && onlyCurrentFolder ? currentPath : '/'
    const scopes: FileToolsScope[] = selectedMounts.map((mountId) => ({
      mountId,
      path: scopePath,
    }))
    const displayScopes: ActiveScanScopeDisplay[] = selectedMounts.map((mountId) => ({
      mountId,
      name: mounts.find((mount) => mount.id === mountId)?.name ?? mountId,
      path: scopePath,
    }))

    setError('')
    setNotice('')
    setResults(null)
    setBreadcrumb([])
    setChildren([])
    setVerifyingSize(null)
    setScanRefresh(refresh)
    setActiveScanScopes(displayScopes)
    setProgress(INITIAL_PROGRESS)
    setWork('scan')

    let createdRunId: string | null = null
    try {
      const id = await koofr.beginFileTools()
      createdRunId = id
      if (sequenceRef.current !== request) {
        await koofr.closeFileTools(id)
        return
      }
      runRef.current = id
      const next = await koofr.scanFileTools(id, scopes, (nextProgress) => {
        if (sequenceRef.current === request) setProgress(nextProgress)
      }, refresh)
      if (sequenceRef.current === request) {
        setResults(next)
        setPreferredTool(destinationTool)
        setView(destinationTool)
        onBackgroundScanCompleteRef.current?.()
      }
    } catch (failure) {
      if (sequenceRef.current === request) {
        if (createdRunId && runRef.current === createdRunId) {
          void koofr.closeFileTools(createdRunId).catch(() => {})
          runRef.current = null
        }
        setError(commandErrorMessage(failure, '扫描失败。'))
        setView('hub')
      }
    } finally {
      if (sequenceRef.current === request) {
        setWork(null)
      }
    }
  }, [mounts, onlyCurrentFolder, preferredTool, selectedMounts, view])

  const cancelScan = useCallback(() => {
    sequenceRef.current += 1
    if (runRef.current) {
      void koofr.closeFileTools(runRef.current).catch(() => {})
      runRef.current = null
    }
    setWork(null)
    setVerifyingSize(null)
    setActiveScanScopes([])
    setView('hub')
    setNotice('扫描已取消。')
  }, [])

  const browse = useCallback(async (next: readonly FileToolsEntry[]) => {
    const id = runRef.current
    if (!id) return
    if (!next.length) {
      setBreadcrumb([])
      setChildren([])
      return
    }
    setWork('browse')
    setError('')
    try {
      const entries = await koofr.fileToolsUsage(id, next[next.length - 1].id)
      if (runRef.current !== id) return
      setBreadcrumb(next)
      setChildren(entries)
    } catch (failure) {
      if (runRef.current === id) setError(commandErrorMessage(failure, '无法显示目录内容。'))
    } finally {
      if (runRef.current === id) setWork(null)
    }
  }, [])

  const verify = useCallback(async (size: number) => {
    const id = runRef.current
    if (!id) return
    setWork('verify')
    setVerifyingSize(size)
    setNotice('')
    setError('')
    setProgress(INITIAL_PROGRESS)
    try {
      const next = await koofr.verifyFileTools(id, size, (nextProgress) => {
        if (runRef.current === id) setProgress(nextProgress)
      })
      if (runRef.current === id) setResults(next)
    } catch (failure) {
      if (runRef.current === id) setError(commandErrorMessage(failure, '核验失败。'))
    } finally {
      if (runRef.current === id) {
        setVerifyingSize(null)
        setWork(null)
      }
    }
  }, [])

  const cancelVerification = useCallback(() => {
    if (runRef.current) void koofr.cancelFileToolsVerification(runRef.current).catch(() => {})
  }, [])

  const remove = useCallback(async (removal: FileToolsRemoval) => {
    const id = runRef.current
    if (!id) return
    const { target, keep } = removal
    setWork('remove')
    setNotice('')
    setError('')
    try {
      const next = await koofr.removeFileToolsEntry(id, target.id, keep?.id)
      onRemoteChangeRef.current()
      if (runRef.current !== id) return
      setResults(next)
      setBreadcrumb([])
      setChildren([])
      setNotice('已删除选中的项目。')
    } catch (failure) {
      if (runRef.current === id) {
        setError(isCommandErrorCode(failure, 'conflict')
          ? '文件或目录已变化。请重新扫描后重试。'
          : commandErrorMessage(failure, '无法删除该项目。请重新扫描后重试。'))
      }
    } finally {
      if (runRef.current === id) setWork(null)
    }
  }, [])

  return {
    view,
    preferredTool,
    mounts,
    mountsLoading,
    sharedUnavailable,
    selectedMounts,
    onlyCurrentFolder,
    results,
    summary,
    breadcrumb,
    children,
    work,
    scanRefresh,
    activeScanScopes,
    verifyingSize,
    progress,
    notice,
    error,
    setView,
    setPreferredTool,
    ensureMountsLoaded,
    refreshMounts,
    toggleMount,
    selectAllOnline,
    selectPrimaryOnly,
    setOnlyCurrentFolder,
    scan,
    cancelScan,
    browse,
    verify,
    cancelVerification,
    remove,
  }
}
