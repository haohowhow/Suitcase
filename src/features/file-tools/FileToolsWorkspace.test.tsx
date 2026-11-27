import { act, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useFileToolsManager } from '../../hooks/useFileToolsManager'
import type { KoofrMount } from '../../types/backend'
import type { FileToolsEntry, FileToolsProgress, FileToolsResults } from '../../types/fileTools'
import { FileToolsWorkspace } from './FileToolsWorkspace'

const api = vi.hoisted(() => ({
  fileToolsMounts: vi.fn(),
  beginFileTools: vi.fn(),
  scanFileTools: vi.fn(),
  fileToolsUsage: vi.fn(),
  verifyFileTools: vi.fn(),
  cancelFileToolsVerification: vi.fn(),
  removeFileToolsEntry: vi.fn(),
  closeFileTools: vi.fn(),
}))

vi.mock('../../services/koofr', () => ({
  koofr: api,
  commandErrorMessage: (_error: unknown, fallback: string) => fallback,
  isCommandErrorCode: () => false,
}))

const baseMounts = [
  { id: 'main', name: 'Koofr', online: true, primary: true },
  { id: 'external', name: 'Dropbox', online: true, primary: false },
]

const workspaceMounts: KoofrMount[] = [
  {
    id: 'main',
    name: 'Koofr',
    mountType: 'device',
    online: true,
    isPrimary: true,
    isShared: false,
    spaceTotal: 10_000,
    spaceUsed: 2_500,
  },
  {
    id: 'external',
    name: 'Dropbox',
    mountType: 'import',
    online: true,
    isPrimary: false,
    isShared: false,
    spaceTotal: 0,
    spaceUsed: 0,
  },
]

function makeEntry(overrides: Partial<FileToolsEntry> = {}): FileToolsEntry {
  return {
    id: { scope: 0, entry: 1 },
    path: '/report.txt',
    size: 1024,
    files: 1,
    directory: false,
    canBrowse: false,
    complete: true,
    removable: false,
    ...overrides,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  api.closeFileTools.mockResolvedValue(undefined)
  api.cancelFileToolsVerification.mockResolvedValue(undefined)
})

describe('FileToolsWorkspace', () => {
  it('shows the read estimate before requesting contents for a selected candidate group', async () => {
    const user = userEvent.setup()
    const file = makeEntry()
    const results: FileToolsResults = {
      cachedFolders: 0,
      mounts: baseMounts,
      roots: [
        makeEntry({ id: { scope: 0, entry: 0 }, path: '/', size: 1024, files: 1, directory: true, canBrowse: true }),
        makeEntry({ id: { scope: 1, entry: 0 }, path: '/', size: 1024, files: 1, directory: true, canBrowse: true }),
      ],
      emptyFolders: [],
      duplicates: [],
      candidates: [{
        size: 1024,
        estimatedBytes: 2048,
        failedEntries: 0,
        entries: [file, { ...file, id: { scope: 1, entry: 1 } }],
      }],
    }
    api.fileToolsMounts.mockResolvedValue({ mounts: baseMounts, sharedUnavailable: false })
    api.beginFileTools.mockResolvedValue('scan-id')
    api.scanFileTools.mockResolvedValue(results)
    api.verifyFileTools.mockResolvedValue({ ...results, candidates: [] })

    render(<FileToolsWorkspace currentMountId="main" currentPath="/" onRemoteChange={vi.fn()} />)
    await screen.findByRole('checkbox', { name: 'Dropbox' })
    await user.click(screen.getByRole('button', { name: '开始扫描' }))
    await user.click(screen.getByRole('tab', { name: '重复文件' }))

    const candidateSection = await screen.findByRole('region', { name: '待核验的候选文件' })
    expect(within(candidateSection).getByText('预计需读取')).toBeTruthy()
    expect(within(candidateSection).getByText(/2\.00 KB/)).toBeTruthy()
    expect(api.verifyFileTools).not.toHaveBeenCalled()

    expect(within(candidateSection).queryByText('Dropbox')).toBeNull()
    await user.click(within(candidateSection).getByRole('button', { name: '查看候选文件' }))
    expect(within(candidateSection).getByText('Koofr')).toBeTruthy()
    expect(within(candidateSection).getByText('Dropbox')).toBeTruthy()

    await user.click(within(candidateSection).getByRole('button', { name: '核验这组' }))
    expect(api.verifyFileTools).toHaveBeenCalledExactlyOnceWith('scan-id', 1024, expect.any(Function))
    expect(await screen.findByText('没有待核验的候选文件。')).toBeTruthy()
  })

  it('lets the user choose a target tool on the entry hub and preserves live metrics when returning', async () => {
    const user = userEvent.setup()
    const emptyDir = makeEntry({
      id: { scope: 0, entry: 2 },
      path: '/Archive/Unused',
      size: 0,
      files: 0,
      directory: true,
      removable: true,
    })
    const results: FileToolsResults = {
      cachedFolders: 4,
      mounts: [baseMounts[0]],
      roots: [
        makeEntry({ id: { scope: 0, entry: 0 }, path: '/Photos', size: 4096, files: 2, directory: true, canBrowse: true }),
      ],
      emptyFolders: [emptyDir],
      duplicates: [],
      candidates: [],
    }
    api.fileToolsMounts.mockResolvedValue({ mounts: [baseMounts[0]], sharedUnavailable: false })
    api.beginFileTools.mockResolvedValue('scan-empty')
    api.scanFileTools.mockResolvedValue(results)

    render(<FileToolsWorkspace currentMountId="main" currentPath="/Photos" onRemoteChange={vi.fn()} />)
    await screen.findByRole('checkbox', { name: 'Koofr' })

    await user.click(screen.getByRole('button', { name: /空文件夹/ }))
    await user.click(screen.getByRole('checkbox', { name: /仅扫描当前文件夹/ }))
    await user.click(screen.getByRole('button', { name: '开始扫描' }))

    expect(api.scanFileTools).toHaveBeenCalledWith(
      'scan-empty',
      [{ mountId: 'main', path: '/Photos' }],
      expect.any(Function),
      false,
    )
    expect(await screen.findByText('Unused')).toBeTruthy()
    expect(screen.getByText('/Archive')).toBeTruthy()

    await user.click(screen.getByRole('button', { name: '工具入口' }))
    expect(await screen.findByText('返回扫描结果')).toBeTruthy()
    expect(screen.getByText('4.00 KB')).toBeTruthy()
  })

  it('supports inline keep-copy selection and confirmed deletion for duplicate files', async () => {
    const user = userEvent.setup()
    const onRemoteChange = vi.fn()
    const copyA = makeEntry({
      id: { scope: 0, entry: 1 },
      path: '/Docs/budget.xlsx',
      size: 5120,
      removable: true,
    })
    const copyB = makeEntry({
      id: { scope: 0, entry: 2 },
      path: '/Backup/budget.xlsx',
      size: 5120,
      removable: true,
    })
    const results: FileToolsResults = {
      cachedFolders: 0,
      mounts: [baseMounts[0]],
      roots: [
        makeEntry({ id: { scope: 0, entry: 0 }, path: '/', size: 10240, files: 2, directory: true, canBrowse: true }),
      ],
      emptyFolders: [],
      duplicates: [[copyA, copyB]],
      candidates: [],
    }
    api.fileToolsMounts.mockResolvedValue({ mounts: [baseMounts[0]], sharedUnavailable: false })
    api.beginFileTools.mockResolvedValue('scan-dup')
    api.scanFileTools.mockResolvedValue(results)
    api.removeFileToolsEntry.mockResolvedValue({ ...results, duplicates: [] })

    render(<FileToolsWorkspace currentMountId="main" currentPath="/" onRemoteChange={onRemoteChange} />)
    await screen.findByRole('checkbox', { name: 'Koofr' })
    await user.click(screen.getByRole('button', { name: /重复文件/ }))
    await user.click(screen.getByRole('button', { name: '开始扫描' }))

    expect(await screen.findByText('请选择要保留的副本')).toBeTruthy()
    const keepRadios = screen.getAllByRole('radio')
    expect(keepRadios).toHaveLength(2)
    expect(screen.queryByRole('button', { name: '删除' })).toBeNull()

    await user.click(keepRadios[0])
    expect(screen.getByText('已选定保留副本')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: '删除' }))

    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByText(/\/Backup\/budget\.xlsx/)).toBeTruthy()
    expect(within(dialog).getByText(/\/Docs\/budget\.xlsx/)).toBeTruthy()
    await user.click(within(dialog).getByRole('button', { name: '删除' }))

    expect(api.removeFileToolsEntry).toHaveBeenCalledExactlyOnceWith('scan-dup', copyB.id, copyA.id)
    expect(onRemoteChange).toHaveBeenCalledTimes(1)
    expect(await screen.findByText('已删除选中的项目。')).toBeTruthy()
  })

  it('drills down into space usage folders and navigates to My files when requested', async () => {
    const user = userEvent.setup()
    const onOpenLocation = vi.fn()
    const root = makeEntry({
      id: { scope: 0, entry: 0 },
      path: '/',
      size: 8192,
      files: 3,
      directory: true,
      canBrowse: true,
    })
    const folder = makeEntry({
      id: { scope: 0, entry: 1 },
      path: '/Videos',
      size: 6144,
      files: 2,
      directory: true,
      canBrowse: true,
    })
    const results: FileToolsResults = {
      cachedFolders: 0,
      mounts: [baseMounts[0]],
      roots: [root],
      emptyFolders: [],
      duplicates: [],
      candidates: [],
    }
    api.fileToolsMounts.mockResolvedValue({ mounts: [baseMounts[0]], sharedUnavailable: false })
    api.beginFileTools.mockResolvedValue('scan-usage')
    api.scanFileTools.mockResolvedValue(results)
    api.fileToolsUsage.mockResolvedValue([folder])

    render(
      <FileToolsWorkspace
        currentMountId="main"
        currentPath="/"
        onRemoteChange={vi.fn()}
        workspaceMounts={workspaceMounts}
        onOpenLocation={onOpenLocation}
      />,
    )
    await screen.findByRole('checkbox', { name: 'Koofr' })
    await user.click(screen.getByRole('button', { name: '开始扫描' }))

    await user.click(await screen.findByRole('button', { name: '查看目录' }))
    expect(api.fileToolsUsage).toHaveBeenCalledExactlyOnceWith('scan-usage', root.id)
    expect(await screen.findByText('Videos')).toBeTruthy()

    await user.click(screen.getByRole('button', { name: '打开 Videos 所在位置' }))
    expect(onOpenLocation).toHaveBeenCalledExactlyOnceWith('main', '/Videos', true)
  })

  it('keeps scanning in the background across tab switches and only cancels manually or on error', async () => {
    const user = userEvent.setup()
    const onBackgroundScanComplete = vi.fn()
    let resolveScan!: (value: FileToolsResults) => void
    let reportProgress!: (progress: FileToolsProgress) => void

    const finishedResults: FileToolsResults = {
      cachedFolders: 2,
      mounts: [baseMounts[0]],
      roots: [
        makeEntry({ id: { scope: 0, entry: 0 }, path: '/', size: 9216, files: 4, directory: true, canBrowse: true }),
      ],
      emptyFolders: [
        makeEntry({ id: { scope: 0, entry: 3 }, path: '/Scratch/Empty', size: 0, files: 0, directory: true, removable: true }),
      ],
      duplicates: [],
      candidates: [],
    }

    api.fileToolsMounts.mockResolvedValue({ mounts: [baseMounts[0]], sharedUnavailable: false })
    api.beginFileTools.mockResolvedValueOnce('scan-bg').mockResolvedValueOnce('scan-cancel').mockResolvedValueOnce('scan-err')
    api.scanFileTools.mockImplementationOnce((_id, _scopes, onProgress) => {
      reportProgress = onProgress
      return new Promise<FileToolsResults>((resolve) => {
        resolveScan = resolve
      })
    })

    function TabHarness({ authState = 'signedIn' }: { readonly authState?: string }) {
      const [activeTab, setActiveTab] = useState<'files' | 'fileTools'>('fileTools')
      const controller = useFileToolsManager({
        authState,
        onRemoteChange: vi.fn(),
        onBackgroundScanComplete: () => {
          if (activeTab !== 'fileTools') onBackgroundScanComplete()
        },
      })
      return (
        <div>
          <button type="button" onClick={() => setActiveTab('files')}>切换到我的文件</button>
          <button type="button" onClick={() => setActiveTab('fileTools')}>切换到文件工具</button>
          {activeTab === 'fileTools' ? (
            <FileToolsWorkspace
              currentMountId="main"
              currentPath="/"
              onRemoteChange={vi.fn()}
              controller={controller}
            />
          ) : (
            <div>我的文件页面</div>
          )}
        </div>
      )
    }

    const { rerender } = render(<TabHarness />)
    await screen.findByRole('checkbox', { name: 'Koofr' })
    await user.click(screen.getByRole('button', { name: '开始扫描' }))
    expect(await screen.findByText('正在扫描云端目录…')).toBeTruthy()

    // Switch away to another tab while scanning is still running.
    await user.click(screen.getByRole('button', { name: '切换到我的文件' }))
    expect(screen.getByText('我的文件页面')).toBeTruthy()
    expect(api.closeFileTools).not.toHaveBeenCalled()

    // Progress updates and completion happen while away from File Tools.
    act(() => {
      reportProgress({ folders: 18, bytes: 0 })
    })
    await user.click(screen.getByRole('button', { name: '切换到文件工具' }))
    expect(screen.getByText('18')).toBeTruthy()

    // Switch away again, let the scan finish in the background, then return.
    await user.click(screen.getByRole('button', { name: '切换到我的文件' }))
    await act(async () => {
      resolveScan(finishedResults)
      await Promise.resolve()
    })
    expect(onBackgroundScanComplete).toHaveBeenCalledTimes(1)
    expect(api.closeFileTools).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: '切换到文件工具' }))
    const summaryStrip = await screen.findByRole('region', { name: '扫描结果总览' })
    expect(within(summaryStrip).getByText('9.00 KB')).toBeTruthy()
    expect(api.fileToolsMounts).toHaveBeenCalledTimes(1)

    // Manual cancel stops the active scan and closes the run.
    api.scanFileTools.mockImplementationOnce(() => new Promise<FileToolsResults>(() => {}))
    await user.click(screen.getByRole('button', { name: '重新扫描' }))
    expect(await screen.findByText('正在扫描云端目录…')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: '取消扫描' }))
    expect(api.closeFileTools).toHaveBeenCalledWith('scan-cancel')
    expect(await screen.findByText('扫描已取消。')).toBeTruthy()

    // Scan error cancels the failed run and surfaces the error on the hub.
    api.scanFileTools.mockRejectedValueOnce(new Error('network'))
    await user.click(screen.getByRole('button', { name: '开始扫描' }))
    expect(await screen.findByRole('alert')).toBeTruthy()
    expect(api.closeFileTools).toHaveBeenCalledWith('scan-err')

    // Signing out resets cleanly.
    rerender(<TabHarness authState="signedOut" />)
  })
})
