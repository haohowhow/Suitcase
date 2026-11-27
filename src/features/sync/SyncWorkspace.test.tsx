import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { KoofrMount, SyncPairSummary } from '../../types/backend'

const service = vi.hoisted(() => ({
  selectSyncFolder: vi.fn(),
  createSyncPair: vi.fn(),
  createOnDemandSyncPair: vi.fn(),
  migrateSyncPairToOnDemand: vi.fn(),
  openSyncRoot: vi.fn(),
  nativeSyncCapability: vi.fn(),
  suggestedOnDemandSyncRoot: vi.fn(),
  setSyncPairPaused: vi.fn(),
  runSyncPairNow: vi.fn(),
  listSyncIssues: vi.fn(),
  listSyncTasks: vi.fn(),
  listSyncFiles: vi.fn(),
  removeSyncPair: vi.fn(),
  listFiles: vi.fn(),
}))
vi.mock('../../services/koofr', () => ({
  commandErrorMessage: (_error: unknown, fallback: string) => fallback,
  koofr: service,
}))
import { SyncWorkspace } from './SyncWorkspace'

const mounts = [
  { id: 'primary', name: 'Koofr', isPrimary: true, online: true, isShared: false, mountType: 'device', spaceTotal: 0, spaceUsed: 0 },
  { id: 'shared', name: 'Shared', isPrimary: false, online: true, isShared: true, mountType: 'device', spaceTotal: 0, spaceUsed: 0 },
  { id: 'drive', name: 'Drive', isPrimary: false, online: true, isShared: false, mountType: 'third-party', spaceTotal: 0, spaceUsed: 0 },
] satisfies KoofrMount[]

const pair: SyncPairSummary = {
  id: 'pair-1',
  localRoot: 'C:/Sync',
  remotePath: '/Sync',
  status: 'idle',
  paused: false,
  syncMode: 'classic',
  rootKind: 'custom',
  migrationState: 'none',
  nativeConnected: false,
  onlineOnlyFiles: 0,
  locallyAvailableFiles: 0,
  pinnedFiles: 0,
  pendingItems: 0,
  pendingBytes: 0,
  completedItems: 0,
  totalItems: 0,
  completedBytes: 0,
  totalBytes: 0,
  lastSyncedAt: null,
  issueCount: 0,
  lastErrorCode: null,
  local: { fileCount: 1, directoryCount: 1, bytes: 100 },
  remote: { fileCount: 1, directoryCount: 1, bytes: 100 },
  upload: { pendingItems: 0, totalItems: 0, completedItems: 0, pendingBytes: 0, totalBytes: 0, completedBytes: 0, bytesPerSecond: 0 },
  download: { pendingItems: 0, totalItems: 0, completedItems: 0, pendingBytes: 0, totalBytes: 0, completedBytes: 0, bytesPerSecond: 0 },
  lastScanAt: null,
}

const sync = {
  pairs: [pair],
  speedSamples: [],
  completionVersion: 0,
  refresh: async () => {},
  setPairPaused: async (pairId: string, paused: boolean) => { await service.setSyncPairPaused(pairId, paused) },
  runPairNow: async (pairId: string) => { await service.runSyncPairNow(pairId) },
  removePair: async (pairId: string) => { await service.removeSyncPair(pairId) },
}

describe('SyncWorkspace', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    service.selectSyncFolder.mockResolvedValue({ grantId: 'grant-1', localPath: 'C:/Sync' })
    service.createSyncPair.mockResolvedValue(pair)
    service.nativeSyncCapability.mockResolvedValue({ supported: true, reason: null, buildNumber: 26100, integrationNumber: 1 })
    service.suggestedOnDemandSyncRoot.mockResolvedValue('C:/Users/Test/Koofr')
    service.setSyncPairPaused.mockResolvedValue(true)
    service.runSyncPairNow.mockResolvedValue(true)
    service.removeSyncPair.mockResolvedValue(true)
    service.openSyncRoot.mockResolvedValue(undefined)
    service.listSyncTasks.mockResolvedValue([])
    service.listSyncFiles.mockResolvedValue({ files: [], hasMore: false })
    service.listSyncIssues.mockResolvedValue([])
    service.listFiles.mockImplementation(async (_mountId: string, path: string) => path === '/'
      ? [{ path: '/Projects', name: 'Projects', entryType: 'dir' }]
      : [])
  })

  const renderWorkspace = () => render(
    <SyncWorkspace mounts={mounts} defaultMountId="primary" defaultRemotePath="/" sync={sync} />,
  )

  it('shows summary and saved pairs without cluttering the page with an open creation form', async () => {
    const user = userEvent.setup()
    renderWorkspace()
    expect(screen.getByText('C:/Sync')).not.toBeNull()
    expect(screen.getByText('全部已同步')).not.toBeNull()
    expect(screen.queryByRole('combobox', { name: 'Koofr 主存储' })).toBeNull()

    await user.click(screen.getByRole('button', { name: '新建同步' }))
    const select = screen.getByRole('combobox', { name: 'Koofr 主存储' })
    expect(select).not.toBeNull()
    expect(screen.getByRole('option', { name: 'Koofr' })).not.toBeNull()
    expect(screen.queryByRole('option', { name: 'Shared' })).toBeNull()
    expect(screen.queryByRole('option', { name: 'Drive' })).toBeNull()
  })

  it('creates a pair from the guided modal using a one-shot local folder grant', async () => {
    const user = userEvent.setup()
    renderWorkspace()
    await user.click(screen.getByRole('button', { name: '新建同步' }))
    await user.click(screen.getByRole('button', { name: '选择本地文件夹' }))
    await user.clear(screen.getByRole('textbox', { name: 'Koofr 文件夹' }))
    await user.type(screen.getByRole('textbox', { name: 'Koofr 文件夹' }), '/Projects')
    await user.click(screen.getByRole('button', { name: '确认创建' }))
    await waitFor(() => expect(service.createSyncPair).toHaveBeenCalledWith('grant-1', 'primary', '/Projects'))
  })

  it('pauses and removes configuration without a file-delete command', async () => {
    const user = userEvent.setup()
    renderWorkspace()
    await user.click(screen.getByRole('button', { name: '暂停同步' }))
    await waitFor(() => expect(service.setSyncPairPaused).toHaveBeenCalledWith('pair-1', true))
    await user.click(screen.getByRole('button', { name: '移除同步' }))
    await user.click(screen.getByRole('button', { name: '确认移除' }))
    await waitFor(() => expect(service.removeSyncPair).toHaveBeenCalledWith('pair-1'))
    expect(service).not.toHaveProperty('deleteEntry')
  })

  it('creates a custom on-demand root through the guided modal', async () => {
    service.createOnDemandSyncPair.mockResolvedValue({ ...pair, syncMode: 'onDemand', nativeConnected: true })
    const user = userEvent.setup()
    renderWorkspace()
    await waitFor(() => expect(service.nativeSyncCapability).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: '新建同步' }))
    await user.click(screen.getByRole('radio', { name: /按需同步（预览）/ }))
    expect(screen.getByRole('radio', { name: '选择 Koofr 文件夹' }).getAttribute('aria-checked')).toBe('true')
    await user.click(screen.getByRole('button', { name: '选择本地文件夹' }))
    await user.clear(screen.getByRole('textbox', { name: 'Koofr 文件夹' }))
    await user.type(screen.getByRole('textbox', { name: 'Koofr 文件夹' }), '/Projects')
    await user.click(screen.getByRole('button', { name: '确认创建' }))
    await waitFor(() => expect(service.createOnDemandSyncPair).toHaveBeenCalledWith('grant-1', 'primary', '/Projects', 'custom'))
  })

  it('lets an on-demand user browse and select a Koofr folder inline in the modal', async () => {
    const user = userEvent.setup()
    renderWorkspace()
    await user.click(screen.getByRole('button', { name: '新建同步' }))
    await user.click(screen.getByRole('radio', { name: /按需同步（预览）/ }))
    const browse = screen.getByRole('button', { name: '浏览 Koofr' }) as HTMLButtonElement
    expect(browse.disabled).toBe(false)
    await user.click(browse)
    await user.click(await screen.findByRole('button', { name: /Projects/ }))
    await waitFor(() => expect(service.listFiles).toHaveBeenLastCalledWith('primary', '/Projects'))
    await user.click(screen.getByRole('button', { name: '选择此目录' }))
    expect((screen.getByRole('textbox', { name: 'Koofr 文件夹' }) as HTMLInputElement).value).toBe('/Projects')
  })

  it('explains the whole-Koofr mode and offers a direct way back to folder selection', async () => {
    const user = userEvent.setup()
    renderWorkspace()
    await user.click(screen.getByRole('button', { name: '新建同步' }))
    await user.click(screen.getByRole('radio', { name: /按需同步（预览）/ }))
    await user.click(screen.getByRole('radio', { name: '同步整个 Koofr' }))
    expect(screen.getByText('整个 Koofr')).not.toBeNull()
    expect(screen.queryByRole('textbox', { name: 'Koofr 文件夹' })).toBeNull()
    await user.click(screen.getByRole('button', { name: '改为选择文件夹' }))
    expect(screen.getByRole('button', { name: '浏览 Koofr' })).not.toBeNull()
  })

  it('disables preview mode when CFAPI capability is unavailable', async () => {
    service.nativeSyncCapability.mockResolvedValue({ supported: false, reason: 'Cloud Files API unavailable', buildNumber: null, integrationNumber: null })
    const user = userEvent.setup()
    renderWorkspace()
    await user.click(screen.getByRole('button', { name: '新建同步' }))
    const option = await screen.findByRole('radio', { name: /按需同步（预览）/ })
    expect(option).toHaveProperty('disabled', true)
  })

  it('offers explicit classic migration and warns about online placeholders on removal', async () => {
    const user = userEvent.setup()
    const classic = renderWorkspace()
    await user.click(await screen.findByRole('button', { name: '切换为按需同步' }))
    await waitFor(() => expect(service.migrateSyncPairToOnDemand).toHaveBeenCalledWith('pair-1', 'grant-1', 'custom'))
    classic.unmount()

    const onDemandSync = { ...sync, pairs: [{ ...pair, syncMode: 'onDemand' as const, nativeConnected: true }] }
    const rendered = render(<SyncWorkspace mounts={mounts} defaultMountId="primary" defaultRemotePath="/" sync={onDemandSync} />)
    await user.click(rendered.getByRole('button', { name: '移除同步' }))
    expect(rendered.getByText(/仅在线的文件会从本机移除/)).not.toBeNull()
    await user.click(rendered.getByRole('button', { name: '确认移除' }))
    await waitFor(() => expect(service.removeSyncPair).toHaveBeenCalledWith('pair-1'))
  })

  it.each(['classic', 'onDemand'] as const)('shows overview progress and searchable file status for %s sync', async (syncMode) => {
    service.listSyncTasks.mockResolvedValue([
      { id: 'task-323', operation: 'upload', direction: 'upload', displayName: 'archive.zip', state: 'running', bytes: 734_003_200 },
      { id: 'task-324', operation: 'upload', direction: 'upload', displayName: 'notes.txt', state: 'queued', bytes: 1024 },
    ])
    const busyPair: SyncPairSummary = {
      ...pair,
      syncMode,
      status: 'syncing',
      nativeConnected: syncMode === 'onDemand',
      pendingItems: 7,
      pendingBytes: 0,
      completedItems: 323,
      totalItems: 330,
      completedBytes: 1_514_612_736,
      totalBytes: 1_514_612_736,
      upload: {
        pendingItems: 7,
        totalItems: 330,
        completedItems: 323,
        pendingBytes: 0,
        totalBytes: 1_514_612_736,
        completedBytes: 1_514_612_736,
        bytesPerSecond: 0,
      },
    }
    const controller = { ...sync, pairs: [busyPair] }
    const user = userEvent.setup()
    render(<SyncWorkspace mounts={mounts} defaultMountId="primary" defaultRemotePath="/" sync={controller} />)
    await user.click(screen.getByRole('button', { name: '详情' }))

    expect(screen.getByText('正在处理上传队列')).not.toBeNull()
    expect(screen.getByText('第 324/330 项 · 剩余 7 项')).not.toBeNull()
    expect(screen.getByText('上传队列')).not.toBeNull()
    expect(screen.getByText('323/330 项 · 剩余 7 项')).not.toBeNull()
    expect(screen.getByRole('progressbar', { name: '当前周期总进度' }).getAttribute('aria-valuenow')).toBe('98')
    expect((await screen.findAllByText('archive.zip')).length).toBeGreaterThan(0)
    expect(service.listSyncTasks).toHaveBeenCalledWith('pair-1')

    service.listSyncFiles.mockResolvedValue({
      files: [
        { relativePath: 'work/notes.txt', kind: 'file', state: 'queued', operation: 'upload', checkedAt: null },
        { relativePath: 'archive/notes.txt', kind: 'file', state: 'failed', operation: 'read_file', checkedAt: null },
      ],
      hasMore: true,
    })
    await user.click(screen.getByRole('tab', { name: '文件状态' }))
    expect(await screen.findByText('work/notes.txt')).not.toBeNull()
    expect(screen.getByText('archive/notes.txt')).not.toBeNull()
    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(service.listSyncFiles).toHaveBeenCalledWith('pair-1', '', 'all', 1))
    await user.selectOptions(screen.getByRole('combobox', { name: '筛选文件状态' }), 'failed')
    await user.type(screen.getByRole('searchbox'), 'archive')
    await waitFor(() => expect(service.listSyncFiles).toHaveBeenCalledWith('pair-1', 'archive', 'failed', 0))

  })

  it('guides an Explorer file failure back to Explorer', async () => {
    service.listSyncIssues.mockResolvedValue([
      { id: 1, relativePath: 'documents/notes.txt', operation: 'read_file', code: 'network_error',
        message: '资源管理器未能下载文件内容。', createdAt: 1730000000000 },
    ])
    const user = userEvent.setup()
    renderWorkspace()
    await user.click(screen.getByRole('button', { name: '详情' }))
    await user.click(screen.getByRole('tab', { name: /问题/ }))
    expect(await screen.findByText('请在资源管理器重新打开文件。')).not.toBeNull()
    const issue = screen.getByRole('listitem')
    await user.click(issue.querySelector('button')!)
    expect(service.openSyncRoot).toHaveBeenCalledWith('pair-1')
  })

  it('opens a synced local root directly in File Explorer for both classic and on-demand pairs', async () => {
    const user = userEvent.setup()
    renderWorkspace()
    await user.click(screen.getByRole('button', { name: '在资源管理器中打开' }))
    await waitFor(() => expect(service.openSyncRoot).toHaveBeenCalledWith('pair-1'))
    expect(service.listFiles).not.toHaveBeenCalled()
  })
})
