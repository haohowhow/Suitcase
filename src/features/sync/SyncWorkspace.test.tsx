import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { KoofrMount, SyncPairSummary } from '../../types/backend'

const service = vi.hoisted(() => ({
  selectSyncFolder: vi.fn(), createSyncPair: vi.fn(), setSyncPairPaused: vi.fn(), runSyncPairNow: vi.fn(), listSyncIssues: vi.fn(), removeSyncPair: vi.fn(),
}))
vi.mock('../../services/koofr', () => ({ commandErrorMessage: (_error: unknown, fallback: string) => fallback, koofr: service }))
import { SyncWorkspace } from './SyncWorkspace'

const mounts = [
  { id: 'primary', name: 'Koofr', isPrimary: true, online: true, isShared: false, mountType: 'device', spaceTotal: 0, spaceUsed: 0 },
  { id: 'shared', name: 'Shared', isPrimary: false, online: true, isShared: true, mountType: 'device', spaceTotal: 0, spaceUsed: 0 },
  { id: 'drive', name: 'Drive', isPrimary: false, online: true, isShared: false, mountType: 'third-party', spaceTotal: 0, spaceUsed: 0 },
] satisfies KoofrMount[]
const pair: SyncPairSummary = {
  id: 'pair-1', localRoot: 'C:/Sync', remotePath: '/Sync', status: 'idle', paused: false,
  pendingItems: 0, pendingBytes: 0, completedItems: 0, totalItems: 0, completedBytes: 0, totalBytes: 0,
  lastSyncedAt: null, issueCount: 0, lastErrorCode: null, local: { fileCount: 1, directoryCount: 1, bytes: 100 }, remote: { fileCount: 1, directoryCount: 1, bytes: 100 },
  upload: { pendingItems: 0, totalItems: 0, completedItems: 0, pendingBytes: 0, totalBytes: 0, completedBytes: 0, bytesPerSecond: 0 }, download: { pendingItems: 0, totalItems: 0, completedItems: 0, pendingBytes: 0, totalBytes: 0, completedBytes: 0, bytesPerSecond: 0 }, lastScanAt: null,
}
const sync = {
  pairs: [pair], speedSamples: [], completionVersion: 0,
  refresh: async () => {}, setPairPaused: async (pairId: string, paused: boolean) => { await service.setSyncPairPaused(pairId, paused) },
  runPairNow: async (pairId: string) => { await service.runSyncPairNow(pairId) }, removePair: async (pairId: string) => { await service.removeSyncPair(pairId) },
}

describe('SyncWorkspace', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    service.selectSyncFolder.mockResolvedValue({ grantId: 'grant-1', localPath: 'C:/Sync' })
    service.createSyncPair.mockResolvedValue(pair)
    service.setSyncPairPaused.mockResolvedValue(true)
    service.runSyncPairNow.mockResolvedValue(true)
    service.removeSyncPair.mockResolvedValue(true)
    vi.spyOn(window, 'confirm').mockReturnValue(true)
  })
  const renderWorkspace = () => render(<SyncWorkspace mounts={mounts} defaultMountId="primary" defaultRemotePath="/" sync={sync} />)

  it('shows only online primary mounts and loads saved pairs', () => {
    renderWorkspace()
    expect(screen.getByText('C:/Sync')).not.toBeNull()
    const select = screen.getByRole('combobox', { name: 'Koofr 主存储' })
    expect(screen.getByRole('option', { name: 'Koofr' })).not.toBeNull()
    expect(screen.queryByRole('option', { name: 'Shared' })).toBeNull()
    expect(screen.queryByRole('option', { name: 'Drive' })).toBeNull()
    expect(select).not.toBeNull()
  })
  it('creates a pair from a one-shot local folder grant', async () => {
    const user = userEvent.setup(); renderWorkspace()
    await user.click(screen.getByRole('button', { name: '选择本地文件夹' }))
    await user.clear(screen.getByRole('textbox', { name: '远端文件夹' }))
    await user.type(screen.getByRole('textbox', { name: '远端文件夹' }), '/Projects')
    await user.click(screen.getByRole('button', { name: '确认创建' }))
    await waitFor(() => expect(service.createSyncPair).toHaveBeenCalledWith('grant-1', 'primary', '/Projects'))
  })
  it('pauses and removes configuration without a file-delete command', async () => {
    const user = userEvent.setup(); renderWorkspace()
    await user.click(screen.getByRole('button', { name: '暂停同步' }))
    await waitFor(() => expect(service.setSyncPairPaused).toHaveBeenCalledWith('pair-1', true))
    await user.click(screen.getByRole('button', { name: '移除同步配置' }))
    await waitFor(() => expect(service.removeSyncPair).toHaveBeenCalledWith('pair-1'))
    expect(service).not.toHaveProperty('deleteEntry')
  })
})
