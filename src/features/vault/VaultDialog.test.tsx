import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { VaultDialog } from './VaultDialog'

const mocks = vi.hoisted(() => ({
  listVaults: vi.fn(),
  unlockVault: vi.fn(),
  listVaultFiles: vi.fn(),
}))

vi.mock('../../services/koofr', () => ({
  commandErrorMessage: (_error: unknown, fallback: string) => fallback,
  isCommandErrorCode: () => false,
  koofr: {
    listVaults: mocks.listVaults,
    unlockVault: mocks.unlockVault,
    listVaultFiles: mocks.listVaultFiles,
  },
}))

const lockedVault = {
  id: 'repo-opaque-id',
  name: '工作资料',
  locked: true,
  added: 0,
  autoLockSeconds: 3600,
}

describe('VaultDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.listVaults.mockResolvedValue([lockedVault])
    mocks.unlockVault.mockResolvedValue({ ...lockedVault, locked: false })
    mocks.listVaultFiles.mockResolvedValue({
      repoId: lockedVault.id,
      repoName: lockedVault.name,
      directoryId: 'root',
      breadcrumbs: [{ id: 'root', name: lockedVault.name }],
      entries: [],
      skippedEntries: 0,
    })
  })

  it('never renders a Safe Key input in the WebView', async () => {
    render(
      <VaultDialog
        mountId="mount-1"
        parentPath="/"
        onClose={vi.fn()}
        onNotice={vi.fn()}
        onTransferStarted={vi.fn()}
        onPreview={vi.fn()}
      />,
    )

    expect(await screen.findByRole('button', { name: /解锁/ })).not.toBeNull()
    expect(document.querySelector('input[type="password"]')).toBeNull()
    expect(screen.queryByRole('textbox', { name: /Safe Key/i })).toBeNull()
    expect(screen.getByPlaceholderText('保险箱名称')).not.toBeNull()
  })

  it('requests native unlock and opens the opaque root handle', async () => {
    const user = userEvent.setup()
    render(
      <VaultDialog
        mountId="mount-1"
        parentPath="/"
        onClose={vi.fn()}
        onNotice={vi.fn()}
        onTransferStarted={vi.fn()}
        onPreview={vi.fn()}
      />,
    )

    await user.click(await screen.findByRole('button', { name: /解锁/ }))

    await waitFor(() => {
      expect(mocks.unlockVault).toHaveBeenCalledWith(lockedVault.id)
      expect(mocks.listVaultFiles).toHaveBeenCalledWith(lockedVault.id, 'root')
    })
    expect(screen.getByText('此加密文件夹为空')).not.toBeNull()
  })

  it('previews a supported file through its opaque entry handle', async () => {
    const user = userEvent.setup()
    const entry = {
      id: 'entry-opaque-id',
      name: 'secret.pdf',
      entryType: 'file',
      modified: 0,
      size: 1024,
      contentType: 'application/pdf',
    } as const
    mocks.listVaultFiles.mockResolvedValue({
      repoId: lockedVault.id,
      repoName: lockedVault.name,
      directoryId: 'root',
      breadcrumbs: [{ id: 'root', name: lockedVault.name }],
      entries: [entry],
      skippedEntries: 0,
    })
    const onPreview = vi.fn()
    render(
      <VaultDialog
        mountId="mount-1"
        parentPath="/"
        onClose={vi.fn()}
        onNotice={vi.fn()}
        onTransferStarted={vi.fn()}
        onPreview={onPreview}
      />,
    )

    await user.click(await screen.findByRole('button', { name: /解锁/ }))
    const row = (await screen.findByText('secret.pdf')).closest('button')
    expect(row).not.toBeNull()
    if (row) await user.dblClick(row)

    expect(onPreview).toHaveBeenCalledWith(lockedVault.id, entry, expect.any(Function))
  })

  it('says how many items could not be decrypted instead of calling the folder empty', async () => {
    const user = userEvent.setup()
    mocks.listVaultFiles.mockResolvedValue({
      repoId: lockedVault.id,
      repoName: lockedVault.name,
      directoryId: 'root',
      breadcrumbs: [{ id: 'root', name: lockedVault.name }],
      entries: [],
      skippedEntries: 2,
    })
    render(
      <VaultDialog
        mountId="mount-1"
        parentPath="/"
        onClose={vi.fn()}
        onNotice={vi.fn()}
        onTransferStarted={vi.fn()}
        onPreview={vi.fn()}
      />,
    )

    await user.click(await screen.findByRole('button', { name: /解锁/ }))

    expect(await screen.findByText(/有 2 个项目无法解密/)).not.toBeNull()
    expect(screen.queryByText('此加密文件夹为空')).toBeNull()
  })
})
