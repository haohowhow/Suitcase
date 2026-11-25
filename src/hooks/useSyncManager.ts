import { useCallback, useEffect, useRef, useState } from 'react'
import { commandErrorMessage, koofr } from '../services/koofr'
import type { SyncPairSummary, SyncStateEvent } from '../types/backend'

export interface SyncSpeedSample {
  readonly pairId: string
  readonly recordedAt: number
  readonly uploadBytesPerSecond: number
  readonly downloadBytesPerSecond: number
}

export interface SyncManagerController {
  readonly pairs: SyncPairSummary[]
  readonly speedSamples: SyncSpeedSample[]
  readonly completionVersion: number
  readonly refresh: () => Promise<void>
  readonly setPairPaused: (pairId: string, paused: boolean) => Promise<void>
  readonly runPairNow: (pairId: string) => Promise<void>
  readonly removePair: (pairId: string) => Promise<void>
}

function mergeEvent(pair: SyncPairSummary, event: SyncStateEvent): SyncPairSummary {
  return {
    ...pair,
    syncMode: event.syncMode,
    rootKind: event.rootKind,
    migrationState: event.migrationState,
    nativeConnected: event.nativeConnected,
    onlineOnlyFiles: event.onlineOnlyFiles,
    locallyAvailableFiles: event.locallyAvailableFiles,
    pinnedFiles: event.pinnedFiles,
    status: event.status,
    pendingItems: event.pendingItems,
    pendingBytes: event.pendingBytes,
    completedItems: event.completedItems,
    totalItems: event.totalItems,
    completedBytes: event.completedBytes,
    totalBytes: event.totalBytes,
    lastSyncedAt: event.lastSyncedAt,
    issueCount: event.issueCount,
    lastErrorCode: event.errorCode,
    local: event.local,
    remote: event.remote,
    upload: event.upload,
    download: event.download,
    lastScanAt: event.lastScanAt,
  }
}

export function useSyncManager(authState: string, showNotice?: (message: string) => void): SyncManagerController {
  const [pairs, setPairs] = useState<SyncPairSummary[]>([])
  const [speedSamples, setSpeedSamples] = useState<SyncSpeedSample[]>([])
  const [completionVersion, setCompletionVersion] = useState(0)
  // Last status seen per pair, kept outside React state so completion detection
  // stays out of state updaters (StrictMode runs those twice) and is not fooled
  // by several events arriving before the next render.
  const lastStatusRef = useRef(new Map<string, string>())

  const refresh = useCallback(async () => {
    try {
      const nextPairs = await koofr.listSyncPairs()
      lastStatusRef.current = new Map(nextPairs.map((pair) => [pair.id, pair.status]))
      setPairs(nextPairs)
    } catch (error) {
      showNotice?.(commandErrorMessage(error, '无法读取同步配置'))
    }
  }, [showNotice])

  useEffect(() => {
    if (authState !== 'signedIn') {
      lastStatusRef.current = new Map()
      setPairs([])
      setSpeedSamples([])
      return
    }
    void refresh()
  }, [authState, refresh])

  useEffect(() => {
    if (authState !== 'signedIn') return
    let disposed = false
    let unlisten: (() => void) | undefined
    void koofr.onSyncState((event) => {
      if (disposed) return
      const previousStatus = lastStatusRef.current.get(event.pairId)
      lastStatusRef.current.set(event.pairId, event.status)
      if (previousStatus !== undefined && previousStatus !== 'idle' && event.status === 'idle') {
        setCompletionVersion((version) => version + 1)
      }
      setPairs((current) => current.map((pair) => pair.id === event.pairId ? mergeEvent(pair, event) : pair))
      const recordedAt = Date.now()
      setSpeedSamples((current) => [
        ...current.filter((sample) => recordedAt - sample.recordedAt < 60_000),
        { pairId: event.pairId, recordedAt, uploadBytesPerSecond: event.upload.bytesPerSecond, downloadBytesPerSecond: event.download.bytesPerSecond },
      ])
    }).then((cleanup) => { if (disposed) cleanup(); else unlisten = cleanup })
    return () => { disposed = true; unlisten?.() }
  }, [authState])

  const hasSpeedSamples = speedSamples.length > 0
  useEffect(() => {
    if (authState !== 'signedIn' || !hasSpeedSamples) return
    const timer = window.setInterval(() => {
      const now = Date.now()
      setSpeedSamples((current) => {
        const next = current.filter((sample) => now - sample.recordedAt < 60_000)
        // Keep the same array when nothing expired. A fresh array every second
        // re-rendered the whole app shell even while idle.
        return next.length === current.length ? current : next
      })
    }, 1000)
    return () => window.clearInterval(timer)
  }, [authState, hasSpeedSamples])

  const setPairPaused = useCallback(async (pairId: string, paused: boolean) => {
    await koofr.setSyncPairPaused(pairId, paused)
    lastStatusRef.current.set(pairId, paused ? 'paused' : 'initializing')
    setPairs((current) => current.map((pair) => pair.id === pairId ? { ...pair, paused, status: paused ? 'paused' : 'initializing' } : pair))
  }, [])
  const runPairNow = useCallback(async (pairId: string) => { await koofr.runSyncPairNow(pairId) }, [])
  const removePair = useCallback(async (pairId: string) => {
    await koofr.removeSyncPair(pairId)
    lastStatusRef.current.delete(pairId)
    setPairs((current) => current.filter((pair) => pair.id !== pairId))
  }, [])

  return { pairs, speedSamples, completionVersion, refresh, setPairPaused, runPairNow, removePair }
}
