import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import type { FileToolsEntry, FileToolsResults } from '../../types/fileTools'
import { FileToolsWorkspace } from './FileToolsWorkspace'

const api = vi.hoisted(() => ({
  fileToolsMounts: vi.fn(),
  beginFileTools: vi.fn(),
  scanFileTools: vi.fn(),
  verifyFileTools: vi.fn(),
  closeFileTools: vi.fn(),
}))

vi.mock('../../services/koofr', () => ({ koofr: api }))

it('shows the read estimate before requesting contents for a selected group', async () => {
  const user = userEvent.setup()
  const mounts = [
    { id: 'main', name: 'Koofr', online: true, primary: true },
    { id: 'external', name: 'Dropbox', online: true, primary: false },
  ]
  const file: FileToolsEntry = {
    id: { scope: 0, entry: 1 },
    path: '/report.txt',
    size: 1024,
    files: 1,
    directory: false,
    canBrowse: false,
    complete: true,
    removable: false,
  }
  const results: FileToolsResults = {
    cachedFolders: 0,
    mounts,
    roots: [],
    emptyFolders: [],
    duplicates: [],
    candidates: [{
      size: 1024,
      estimatedBytes: 2048,
      failedEntries: 0,
      entries: [file, { ...file, id: { scope: 1, entry: 1 } }],
    }],
  }
  api.fileToolsMounts.mockResolvedValue({ mounts, sharedUnavailable: false })
  api.beginFileTools.mockResolvedValue('scan-id')
  api.scanFileTools.mockResolvedValue(results)
  api.verifyFileTools.mockResolvedValue({ ...results, candidates: [] })
  api.closeFileTools.mockResolvedValue(undefined)

  render(<FileToolsWorkspace currentMountId="main" currentPath="/" onRemoteChange={vi.fn()} />)
  await screen.findByRole('checkbox', { name: 'Dropbox' })
  await user.click(screen.getByRole('button', { name: '开始扫描' }))
  await user.click(screen.getByRole('tab', { name: '重复文件' }))
  expect(await screen.findByText('预计需读取')).toBeTruthy()
  expect(screen.getByText(/2\.00 KB/)).toBeTruthy()
  expect(api.verifyFileTools).not.toHaveBeenCalled()

  await user.click(screen.getByRole('button', { name: '核验这组' }))
  expect(api.verifyFileTools).toHaveBeenCalledExactlyOnceWith('scan-id', 1024, expect.any(Function))
  expect(await screen.findByText('没有待核验的候选文件。')).toBeTruthy()
})
