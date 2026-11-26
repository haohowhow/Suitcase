import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { KoofrMount } from '../../types/backend'
import { VaultWorkspace } from './VaultWorkspace'

const mocks = vi.hoisted(() => ({
  listVaults: vi.fn(),
  unlockVault: vi.fn(),
  createVault: vi.fn(),
  listVaultFiles: vi.fn(),
  listFiles: vi.fn(),
}))

vi.mock('../../services/koofr', () => ({
  commandErrorMessage: (_error: unknown, fallback: string) => fallback,
  isCommandErrorCode: () => false,
  koofr: {
    listVaults: mocks.listVaults,
    unlockVault: mocks.unlockVault,
    createVault: mocks.createVault,
    listVaultFiles: mocks.listVaultFiles,
    listFiles: mocks.listFiles,
  },
}))

const mounts: KoofrMount[] = [{
  id: 'mount-1',
  name: 'Koofr',
  mountType: 'device',
  spaceTotal: 1000,
  spaceUsed: 200,
  online: true,
  isPrimary: true,
  isShared: false,
}]

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

describe('VaultWorkspace', () => {
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
    mocks.listFiles.mockResolvedValue([])
  })

  it('opens an in-app Safe Key modal to unlock and renders the decrypted workspace', async () => {
    const user = userEvent.setup()
    render(
      <VaultWorkspace
        mounts={mounts}
        defaultMountId="mount-1"
        defaultParentPath="/"
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

  it('creates a vault through the setup modal and Safe Key confirmation modal', async () => {
    const user = userEvent.setup()
    render(
      <VaultWorkspace
        mounts={mounts}
        defaultMountId="mount-1"
        defaultParentPath="/Docs"
        onNotice={vi.fn()}
        onTransferStarted={vi.fn()}
        onPreview={vi.fn()}
      />,
    )

    await screen.findByText('工作资料')
    await user.click(screen.getByRole('button', { name: '新建保险箱' }))
    const nameInput = await screen.findByPlaceholderText('输入新保险箱名称')
    await user.type(nameInput, '机密归档')
    await user.click(screen.getByRole('button', { name: '下一步' }))

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
      <VaultWorkspace
        mounts={mounts}
        defaultMountId="mount-1"
        defaultParentPath="/"
        onNotice={vi.fn()}
        onTransferStarted={vi.fn()}
        onPreview={onPreview}
      />,
    )

    await unlockViaModal(user)
    const rowButton = (await screen.findByText('secret.pdf')).closest('button')
    expect(rowButton).not.toBeNull()
    if (rowButton) await user.dblClick(rowButton)

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
      <VaultWorkspace
        mounts={mounts}
        defaultMountId="mount-1"
        defaultParentPath="/"
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
