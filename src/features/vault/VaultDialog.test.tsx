import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { VaultDialog } from './VaultDialog'

const mocks = vi.hoisted(() => ({
  listVaults: vi.fn(),
  unlockVault: vi.fn(),
  createVault: vi.fn(),
  listVaultFiles: vi.fn(),
}))

vi.mock('../../services/koofr', () => ({
  commandErrorMessage: (_error: unknown, fallback: string) => fallback,
  isCommandErrorCode: () => false,
  koofr: {
    listVaults: mocks.listVaults,
    unlockVault: mocks.unlockVault,
    createVault: mocks.createVault,
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

async function unlockViaModal(user: ReturnType<typeof userEvent.setup>, safeKey = 'test-safe-key') {
  await user.click(await screen.findByRole('button', { name: /^解锁$/ }))
  const keyInput = await screen.findByLabelText('Safe Key')
  await user.type(keyInput, safeKey)
  const unlockButtons = screen.getAllByRole('button', { name: /^解锁$/ })
  await user.click(unlockButtons[unlockButtons.length - 1])
}

describe('VaultDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.listVaults.mockResolvedValue([lockedVault])
    mocks.unlockVault.mockResolvedValue({ ...lockedVault, locked: false })
    mocks.createVault.mockResolvedValue({ ...lockedVault, id: 'repo-new', name: '机密归档', locked: false })
    mocks.listVaultFiles.mockResolvedValue({
      repoId: lockedVault.id,
      repoName: lockedVault.name,
      directoryId: 'root',
      breadcrumbs: [{ id: 'root', name: lockedVault.name }],
      entries: [],
      skippedEntries: 0,
    })
  })

  it('opens an in-app Safe Key modal to unlock and opens the opaque root handle', async () => {
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

    await unlockViaModal(user, 'my-vault-passphrase')

    await waitFor(() => {
      expect(mocks.unlockVault).toHaveBeenCalledWith(lockedVault.id, 'my-vault-passphrase')
      expect(mocks.listVaultFiles).toHaveBeenCalledWith(lockedVault.id, 'root')
    })
    expect(screen.getByText('此加密文件夹为空')).not.toBeNull()
  })

  it('validates and submits Safe Key in a unified create modal', async () => {
    const user = userEvent.setup()
    render(
      <VaultDialog
        mountId="mount-1"
        parentPath="/Docs"
        onClose={vi.fn()}
        onNotice={vi.fn()}
        onTransferStarted={vi.fn()}
        onPreview={vi.fn()}
      />,
    )

    const nameInput = await screen.findByPlaceholderText('输入新保险箱名称')
    await user.type(nameInput, '机密归档')
    await user.click(screen.getByRole('button', { name: /^创建$/ }))

    const keyInput = await screen.findByLabelText(/Safe Key（至少 8 个字符）/)
    const confirmInput = screen.getByLabelText('确认 Safe Key')
    const submitButton = screen.getByRole('button', { name: '创建并解锁' })

    await user.type(keyInput, 'short')
    expect((submitButton as HTMLButtonElement).disabled).toBe(true)

    await user.clear(keyInput)
    await user.type(keyInput, 'strong-passphrase')
    await user.type(confirmInput, 'different-passphrase')
    expect((submitButton as HTMLButtonElement).disabled).toBe(true)

    await user.clear(confirmInput)
    await user.type(confirmInput, 'strong-passphrase')
    expect((submitButton as HTMLButtonElement).disabled).toBe(false)

    await user.click(submitButton)

    await waitFor(() => {
      expect(mocks.createVault).toHaveBeenCalledWith('mount-1', '/Docs', '机密归档', 'strong-passphrase')
      expect(mocks.listVaultFiles).toHaveBeenCalledWith('repo-new', 'root')
    })
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

    await unlockViaModal(user)
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

    await unlockViaModal(user)

    expect(await screen.findByText(/有 2 个项目无法解密/)).not.toBeNull()
    expect(screen.queryByText('此加密文件夹为空')).toBeNull()
  })
})
