import { act, renderHook, waitFor } from '@testing-library/react'
import { StrictMode, createElement, useEffect, type ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SyncPairSummary, SyncStateEvent } from '../types/backend'

const service = vi.hoisted(() => ({
  listSyncPairs: vi.fn(),
  onSyncState: vi.fn(),
  setSyncPairPaused: vi.fn(),
  runSyncPairNow: vi.fn(),
  removeSyncPair: vi.fn(),
}))
vi.mock('../services/koofr', () => ({ commandErrorMessage: (_error: unknown, fallback: string) => fallback, koofr: service }))
import { useSyncManager } from './useSyncManager'

const directionStats = { pendingItems: 0, totalItems: 0, completedItems: 0, pendingBytes: 0, totalBytes: 0, completedBytes: 0, bytesPerSecond: 0 }
const pair: SyncPairSummary = {
  id: 'pair-1', localRoot: 'C:/Sync', remotePath: '/Sync', status: 'syncing', paused: false,
  syncMode: 'classic', rootKind: 'custom', migrationState: 'none', nativeConnected: false,
  onlineOnlyFiles: 0, locallyAvailableFiles: 0, pinnedFiles: 0,
  pendingItems: 1, pendingBytes: 0, completedItems: 0, totalItems: 1, completedBytes: 0, totalBytes: 0,
  lastSyncedAt: null, issueCount: 0, lastErrorCode: null, local: { fileCount: 0, directoryCount: 0, bytes: 0 }, remote: { fileCount: 0, directoryCount: 0, bytes: 0 },
  upload: directionStats, download: directionStats, lastScanAt: null,
}

function eventFor(status: string): SyncStateEvent {
  return {
    pairId: pair.id, syncMode: pair.syncMode, rootKind: pair.rootKind, migrationState: pair.migrationState,
    nativeConnected: false, onlineOnlyFiles: 0, locallyAvailableFiles: 0, pinnedFiles: 0, status,
    pendingItems: 0, pendingBytes: 0, completedItems: 1, totalItems: 1, completedBytes: 0, totalBytes: 0,
    lastSyncedAt: null, issueCount: 0, errorCode: null, local: pair.local, remote: pair.remote,
    upload: directionStats, download: directionStats, lastScanAt: null,
  } as SyncStateEvent
}

describe('useSyncManager', () => {
  let emit: (event: SyncStateEvent) => void = () => {}

  beforeEach(() => {
    vi.clearAllMocks()
    service.listSyncPairs.mockResolvedValue([pair])
    service.onSyncState.mockImplementation(async (handler: (event: SyncStateEvent) => void) => {
      emit = handler
      return () => {}
    })
  })

  it('counts one completion per transition to idle, even in StrictMode and with bursts of events', async () => {
    const wrapper = ({ children }: { children: ReactNode }) => createElement(StrictMode, null, children)
    const { result } = renderHook(() => useSyncManager('signedIn'), { wrapper })
    await waitFor(() => expect(result.current.pairs).toHaveLength(1))
    await waitFor(() => expect(service.onSyncState).toHaveBeenCalled())

    act(() => {
      emit(eventFor('idle'))
      emit(eventFor('idle'))
    })
    expect(result.current.completionVersion).toBe(1)
    expect(result.current.pairs[0].status).toBe('idle')

    act(() => {
      emit(eventFor('syncing'))
      emit(eventFor('idle'))
    })
    expect(result.current.completionVersion).toBe(2)
  })

  it('stays idle between events instead of re-rendering every second', async () => {
    vi.useFakeTimers()
    try {
      // Count committed renders: React may call the component once more before
      // it bails out of an unchanged state, but it commits nothing in that case.
      let commits = 0
      const { result } = renderHook(() => {
        useEffect(() => {
          commits += 1
        })
        return useSyncManager('signedIn')
      })
      await act(async () => {
        await vi.advanceTimersByTimeAsync(1)
      })
      expect(result.current.pairs).toHaveLength(1)
      const settled = commits

      // The sample clean-up timer ticks once a second. With no samples to expire
      // it must not hand React a new array, or the whole app shell re-renders.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(5_000)
      })

      expect(commits).toBe(settled)
    } finally {
      vi.useRealTimers()
    }
  })
})
