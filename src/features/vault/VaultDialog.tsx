import {
  ArrowLeft,
  Copy,
  Download,
  Eye,
  Folder,
  FolderInput,
  FolderPlus,
  KeyRound,
  Lock,
  LockOpen,
  Pencil,
  RefreshCw,
  ShieldCheck,
  ShieldPlus,
  Trash2,
  Upload,
} from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { FileTypeIcon } from '../../components/FileTypeIcon'
import { Modal } from '../../components/Modal'
import { isPreviewableFile } from '../../preview'
import { commandErrorMessage, isCommandErrorCode, koofr } from '../../services/koofr'
import type { VaultDirectory, VaultEntry, VaultSummary } from '../../types/backend'
import type { VaultTransferSpec } from '../../types/vault'
import { fileKindByName, formatBytes, formatModified } from '../../utils/format'
import { VaultSafeKeyModal } from './VaultSafeKeyModal'
import type { VaultSafeKeyPrompt } from './VaultSafeKeyModal'

interface VaultDialogProps {
  readonly mountId: string
  readonly parentPath: string
  readonly onClose: () => void
  readonly onNotice: (message: string) => void
  readonly onTransferStarted: (transfer: VaultTransferSpec) => void
  readonly onPreview: (repoId: string, entry: VaultEntry, onDownload: () => void) => void
}

interface Relocation {
  readonly entry: VaultEntry
  readonly isMove: boolean
}

export function VaultDialog({
  mountId,
  parentPath,
  onClose,
  onNotice,
  onTransferStarted,
  onPreview,
}: VaultDialogProps) {
  const [vaults, setVaults] = useState<VaultSummary[]>([])
  const [directory, setDirectory] = useState<VaultDirectory | null>(null)
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [createName, setCreateName] = useState('')
  const [folderName, setFolderName] = useState('')
  const [creatingFolder, setCreatingFolder] = useState(false)
  const [renameName, setRenameName] = useState('')
  const [confirmingDelete, setConfirmingDelete] = useState(false)
  const [relocation, setRelocation] = useState<Relocation | null>(null)
  const [keyPrompt, setKeyPrompt] = useState<VaultSafeKeyPrompt | null>(null)
  const [keyPromptError, setKeyPromptError] = useState('')
  const [keyPromptBusy, setKeyPromptBusy] = useState(false)
  const directoryRef = useRef<VaultDirectory | null>(null)

  useEffect(() => {
    directoryRef.current = directory
  }, [directory])

  const reportError = (cause: unknown, fallback: string) => {
    if (!isCommandErrorCode(cause, 'cancelled')) {
      setError(commandErrorMessage(cause, fallback))
    }
  }

  const loadVaults = async () => {
    setLoading(true)
    setError('')
    try {
      const next = await koofr.listVaults()
      setVaults(next)
      if (directory && !next.some((vault) => vault.id === directory.repoId && !vault.locked)) {
        setDirectory(null)
      }
    } catch (cause) {
      reportError(cause, '无法读取保险箱列表。')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    void loadVaults()
    // Initial load only. Later refreshes are explicit to avoid replacing active opaque handles.
  }, [])

  const openDirectory = async (repoId: string, directoryId: string) => {
    setLoading(true)
    setError('')
    try {
      const next = await koofr.listVaultFiles(repoId, directoryId)
      setDirectory(next)
      setSelectedIds(new Set())
    } catch (cause) {
      if (isCommandErrorCode(cause, 'vault_locked')) {
        setDirectory(null)
        await loadVaults()
      }
      reportError(cause, '无法读取保险箱内容。')
    } finally {
      setLoading(false)
    }
  }

  const run = async (operation: () => Promise<void>, fallback: string) => {
    setBusy(true)
    setError('')
    try {
      await operation()
    } catch (cause) {
      reportError(cause, fallback)
    } finally {
      setBusy(false)
    }
  }

  const openKeyPrompt = (prompt: VaultSafeKeyPrompt) => {
    setError('')
    setKeyPromptError('')
    setKeyPrompt(prompt)
  }

  const closeKeyPrompt = () => {
    if (keyPromptBusy) return
    setKeyPrompt(null)
    setKeyPromptError('')
  }

  const submitKeyPrompt = async (safeKey: string, confirmation: string) => {
    if (!keyPrompt) return
    setKeyPromptBusy(true)
    setKeyPromptError('')
    try {
      if (keyPrompt.kind === 'unlock') {
        await koofr.unlockVault(keyPrompt.vault.id, safeKey)
        const repoId = keyPrompt.vault.id
        setKeyPrompt(null)
        await openDirectory(repoId, 'root')
        setVaults(await koofr.listVaults())
      } else if (keyPrompt.kind === 'create') {
        const created = await koofr.createVault(mountId, keyPrompt.parentPath, keyPrompt.name, safeKey)
        setCreateName('')
        setKeyPrompt(null)
        setVaults(await koofr.listVaults())
        await openDirectory(created.id, 'root')
      } else if (keyPrompt.kind === 'export') {
        const exported = await koofr.exportVaultRcloneConfig(keyPrompt.vault.id, safeKey)
        setKeyPrompt(null)
        if (exported) onNotice('配置已导出，请妥善保管该文件。')
      } else {
        const next = await koofr.removeVault(keyPrompt.vault.id, confirmation, safeKey)
        const removedId = keyPrompt.vault.id
        setKeyPrompt(null)
        setVaults(next)
        if (directory?.repoId === removedId) setDirectory(null)
        onNotice('已从列表中移除保险箱；Koofr 中的加密文件仍然保留。')
      }
    } catch (cause) {
      if (isCommandErrorCode(cause, 'cancelled')) {
        setKeyPrompt(null)
        return
      }
      const fallback =
        keyPrompt.kind === 'unlock'
          ? '无法解锁保险箱，请检查 Safe Key 是否正确。'
          : keyPrompt.kind === 'create'
            ? '无法创建保险箱。'
            : keyPrompt.kind === 'export'
              ? '无法导出 rclone 配置，请检查 Safe Key 是否正确。'
              : '无法移除保险箱，请检查 Safe Key 是否正确。'
      setKeyPromptError(commandErrorMessage(cause, fallback))
    } finally {
      setKeyPromptBusy(false)
    }
  }

  const lock = (repoId: string) => run(async () => {
    setVaults(await koofr.lockVault(repoId))
    setDirectory(null)
    setRelocation(null)
  }, '无法锁定保险箱。')

  const beginCreateVault = () => {
    const name = createName.trim()
    if (!mountId || !name) return
    openKeyPrompt({ kind: 'create', name, parentPath })
  }

  const importVault = () => run(async () => {
    setVaults(await koofr.importVaultRcloneConfig())
    onNotice('保险箱已导入。')
  }, '无法导入保险箱配置。')

  const refreshDirectory = () => {
    if (directory) void openDirectory(directory.repoId, directory.directoryId)
    else void loadVaults()
  }

  const selected = directory?.entries.filter((entry) => selectedIds.has(entry.id)) ?? []
  const singleSelected = selected.length === 1 ? selected[0] : null
  const previewableSelected = singleSelected && isPreviewableFile(singleSelected)
    ? singleSelected
    : null

  const createFolder = () => run(async () => {
    if (!directory || !folderName.trim()) return
    await koofr.createVaultFolder(directory.repoId, directory.directoryId, folderName.trim())
    setFolderName('')
    setCreatingFolder(false)
    await openDirectory(directory.repoId, directory.directoryId)
  }, '无法创建加密文件夹。')

  const renameEntry = () => run(async () => {
    if (!directory || !singleSelected || !renameName.trim()) return
    await koofr.renameVaultEntry(directory.repoId, singleSelected.id, renameName.trim())
    setRenameName('')
    await openDirectory(directory.repoId, directory.directoryId)
  }, '无法重命名加密项目。')

  const deleteEntries = () => run(async () => {
    if (!directory || selected.length === 0) return
    setConfirmingDelete(false)
    await koofr.deleteVaultEntries(directory.repoId, selected.map((entry) => entry.id))
    await openDirectory(directory.repoId, directory.directoryId)
  }, '无法删除加密项目。')

  const completeRelocation = () => run(async () => {
    if (!directory || !relocation) return
    await koofr.relocateVaultEntry(
      directory.repoId,
      relocation.entry.id,
      directory.directoryId,
      relocation.isMove,
    )
    const source = relocation
    setRelocation(null)
    await openDirectory(directory.repoId, directory.directoryId)
    onNotice(`${source.entry.name} 已${source.isMove ? '移动' : '复制'}到目标文件夹。`)
  }, relocation?.isMove ? '无法移动加密项目。' : '无法复制加密项目。')

  const upload = () => run(async () => {
    if (!directory) return
    const selection = await koofr.selectUploadFile()
    if (!selection) return
    const transfer = koofr.uploadVaultFile(directory.repoId, directory.directoryId, selection.grantId)
    const sourceRepo = directory.repoId
    const sourceDirectory = directory.directoryId
    const result = transfer.result.then(async (value) => {
      const current = directoryRef.current
      if (current?.repoId === sourceRepo && current.directoryId === sourceDirectory) {
        await openDirectory(sourceRepo, sourceDirectory)
      }
      return value
    })
    onTransferStarted({
      transferId: transfer.transferId,
      name: selection.fileName,
      direction: 'upload',
      totalBytes: null,
      recoveryKind: 'restart',
      localPath: selection.localPath,
      result,
    })
  }, '无法开始加密上传。')

  const downloadEntry = (entry: VaultEntry) => run(async () => {
    if (!directory || entry.entryType !== 'file') return
    const selection = await koofr.selectDownloadLocation(entry.name)
    if (!selection) return
    const transfer = koofr.downloadVaultFile(
      directory.repoId,
      entry.id,
      entry.name,
      selection.grantId,
    )
    onTransferStarted({
      transferId: transfer.transferId,
      name: entry.name,
      direction: 'download',
      totalBytes: entry.size,
      recoveryKind: 'byte_resume',
      localPath: selection.localPath,
      result: transfer.result,
    })
  }, '无法开始加密下载。')

  return (
    <>
      <Modal title="Koofr Vault" actionLabel="关闭" extraWide onClose={onClose}>
        <div className="vault-workspace">
          {error ? <div className="vault-error" role="alert">{error}</div> : null}

          {!directory ? (
            <>
              <div className="vault-hero">
                <div className="vault-hero__lead">
                  <span className="vault-hero__icon" aria-hidden="true">
                    <ShieldCheck size={19} strokeWidth={1.85} />
                  </span>
                  <div className="vault-hero__copy">
                    <strong>端到端加密保险箱</strong>
                    <span>文件名与内容均在本地加密后再上传，兼容官方 Koofr Vault 与 rclone。</span>
                  </div>
                </div>
                <div className="vault-toolbar">
                  <button className="secondary-button" type="button" disabled={busy} onClick={() => void loadVaults()}>
                    <RefreshCw size={14} strokeWidth={1.8} />刷新
                  </button>
                  <button className="secondary-button" type="button" disabled={busy} onClick={() => void importVault()}>
                    <FolderInput size={14} strokeWidth={1.8} />导入 rclone 配置
                  </button>
                </div>
              </div>

              <div className="vault-section-header">
                <h3>我的保险箱</h3>
                {!loading ? <span className="vault-count">{vaults.length} 个</span> : null}
              </div>

              <div className="vault-list">
                {loading ? <p className="vault-muted vault-muted--padded">正在读取保险箱…</p> : null}
                {!loading && vaults.length === 0 ? (
                  <div className="vault-empty">
                    <span className="vault-empty__icon" aria-hidden="true">
                      <Lock size={20} strokeWidth={1.75} />
                    </span>
                    <strong>还没有保险箱</strong>
                    <span>可在下方新建加密保险箱，或点击右上角导入已有的 rclone 配置文件。</span>
                  </div>
                ) : null}
                {vaults.map((vault) => (
                  <article className="vault-card" key={vault.id}>
                    <div className="vault-card__info">
                      <span
                        className={`vault-card__icon${vault.locked ? '' : ' vault-card__icon--unlocked'}`}
                        aria-hidden="true"
                      >
                        {vault.locked ? (
                          <Lock size={17} strokeWidth={1.85} />
                        ) : (
                          <LockOpen size={17} strokeWidth={1.85} />
                        )}
                      </span>
                      <div className="vault-card__meta">
                        <strong translate="no">{vault.name}</strong>
                        <span
                          className={`vault-status-pill${vault.locked ? '' : ' vault-status-pill--unlocked'}`}
                        >
                          {vault.locked
                            ? '已锁定'
                            : `已解锁 · ${Math.round(vault.autoLockSeconds / 60)} 分钟后自动锁定`}
                        </span>
                      </div>
                    </div>
                    <div className="vault-card__actions">
                      {vault.locked ? (
                        <button
                          className="primary-button"
                          type="button"
                          disabled={busy}
                          onClick={() => openKeyPrompt({ kind: 'unlock', vault })}
                        >
                          <KeyRound size={14} strokeWidth={1.8} />解锁
                        </button>
                      ) : (
                        <>
                          <button
                            className="primary-button"
                            type="button"
                            disabled={busy}
                            onClick={() => void openDirectory(vault.id, 'root')}
                          >
                            <Folder size={14} strokeWidth={1.8} />打开
                          </button>
                          <button
                            className="secondary-button"
                            type="button"
                            disabled={busy}
                            onClick={() => void lock(vault.id)}
                          >
                            <Lock size={14} strokeWidth={1.8} />锁定
                          </button>
                        </>
                      )}
                      <button
                        className="secondary-button"
                        type="button"
                        disabled={busy}
                        onClick={() => openKeyPrompt({ kind: 'export', vault })}
                      >
                        导出配置
                      </button>
                      <button
                        className="danger-button"
                        type="button"
                        disabled={busy}
                        onClick={() => openKeyPrompt({ kind: 'remove', vault })}
                      >
                        移除
                      </button>
                    </div>
                  </article>
                ))}
              </div>

              <div className="vault-create">
                <div className="vault-create__header">
                  <span>在当前 Koofr 文件夹新建保险箱</span>
                  <code className="vault-create__path" translate="no">{parentPath || '/'}</code>
                </div>
                <div className="vault-create__row">
                  <input
                    aria-label="在当前 Koofr 文件夹创建保险箱"
                    value={createName}
                    maxLength={255}
                    placeholder="输入新保险箱名称"
                    onChange={(event) => setCreateName(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' && !busy && mountId && createName.trim()) {
                        event.preventDefault()
                        beginCreateVault()
                      }
                    }}
                  />
                  <button
                    className="primary-button"
                    type="button"
                    disabled={busy || !mountId || !createName.trim()}
                    onClick={beginCreateVault}
                  >
                    <ShieldPlus size={15} strokeWidth={1.8} />创建
                  </button>
                </div>
              </div>
            </>
          ) : (
            <>
              <div className="vault-toolbar vault-toolbar--browser">
                <button className="secondary-button" type="button" disabled={busy} onClick={() => setDirectory(null)}>
                  <ArrowLeft size={14} strokeWidth={1.8} />保险箱列表
                </button>
                <nav className="vault-breadcrumb" aria-label="保险箱路径">
                  {directory.breadcrumbs.map((crumb, index) => (
                    <span key={crumb.id}>
                      {index > 0 ? <i>/</i> : null}
                      <button translate="no" type="button" disabled={busy || crumb.id === directory.directoryId} onClick={() => void openDirectory(directory.repoId, crumb.id)}>
                        {crumb.name}
                      </button>
                    </span>
                  ))}
                </nav>
                <button className="secondary-button" type="button" disabled={busy} aria-label="刷新" title="刷新" onClick={refreshDirectory}>
                  <RefreshCw size={14} strokeWidth={1.8} />
                  <span className="sr-only">刷新</span>
                </button>
                <button className="secondary-button" type="button" disabled={busy} onClick={() => void lock(directory.repoId)}>
                  <Lock size={14} strokeWidth={1.8} />锁定
                </button>
              </div>

              {relocation ? (
                <div className="vault-relocation">
                  <span>选择目标文件夹：<bdi translate="no">{relocation.entry.name}</bdi></span>
                  <button className="primary-button" type="button" disabled={busy} onClick={() => void completeRelocation()}>
                    {relocation.isMove ? <FolderInput size={14} strokeWidth={1.8} /> : <Copy size={14} strokeWidth={1.8} />}
                    {relocation.isMove ? '移动到此处' : '复制到此处'}
                  </button>
                  <button className="secondary-button" type="button" onClick={() => setRelocation(null)}>取消</button>
                </div>
              ) : (
                <>
                  <div className="vault-toolbar">
                    <button className="primary-button" type="button" disabled={busy} onClick={() => void upload()}>
                      <Upload size={14} strokeWidth={1.85} />加密上传
                    </button>
                    <button className="secondary-button" type="button" disabled={busy} onClick={() => {
                      setCreatingFolder((open) => !open)
                      setRenameName('')
                    }}>
                      <FolderPlus size={14} strokeWidth={1.8} />新建文件夹
                    </button>
                    <button className="secondary-button" type="button" disabled={busy || !singleSelected || singleSelected.entryType !== 'file'} onClick={() => singleSelected && void downloadEntry(singleSelected)}>
                      <Download size={14} strokeWidth={1.8} />解密下载
                    </button>
                    <button className="secondary-button" type="button" disabled={busy || !previewableSelected} onClick={() => {
                      if (directory && previewableSelected) {
                        onPreview(directory.repoId, previewableSelected, () => void downloadEntry(previewableSelected))
                      }
                    }}>
                      <Eye size={14} strokeWidth={1.8} />预览
                    </button>
                    <button className="secondary-button" type="button" disabled={busy || !singleSelected} onClick={() => {
                      if (singleSelected) {
                        setCreatingFolder(false)
                        setRenameName(singleSelected.name)
                      }
                    }}>
                      <Pencil size={14} strokeWidth={1.8} />重命名
                    </button>
                    <button className="secondary-button" type="button" disabled={busy || !singleSelected} onClick={() => singleSelected && setRelocation({ entry: singleSelected, isMove: true })}>
                      <FolderInput size={14} strokeWidth={1.8} />移动
                    </button>
                    <button className="secondary-button" type="button" disabled={busy || !singleSelected} onClick={() => singleSelected && setRelocation({ entry: singleSelected, isMove: false })}>
                      <Copy size={14} strokeWidth={1.8} />复制
                    </button>
                    <button className="danger-button" type="button" disabled={busy || selected.length === 0} onClick={() => setConfirmingDelete(true)}>
                      <Trash2 size={14} strokeWidth={1.8} />删除
                    </button>
                  </div>

                  {confirmingDelete && selected.length > 0 ? (
                    <div className="vault-relocation vault-relocation--danger">
                      <span>确定删除所选 {selected.length} 个加密项目吗？</span>
                      <button className="danger-button" type="button" disabled={busy} onClick={() => void deleteEntries()}>
                        确认删除
                      </button>
                      <button className="secondary-button" type="button" disabled={busy} onClick={() => setConfirmingDelete(false)}>
                        取消
                      </button>
                    </div>
                  ) : null}

                  {creatingFolder || renameName ? (
                    <div className="vault-inline-forms">
                      {creatingFolder ? (
                        <label>
                          <span>新建文件夹</span>
                          <input
                            autoFocus
                            value={folderName}
                            placeholder="文件夹名称"
                            onChange={(event) => setFolderName(event.target.value)}
                            onKeyDown={(event) => {
                              if (event.key === 'Enter' && !busy && folderName.trim()) {
                                event.preventDefault()
                                void createFolder()
                              } else if (event.key === 'Escape') {
                                setCreatingFolder(false)
                                setFolderName('')
                              }
                            }}
                          />
                          <button className="primary-button" type="button" disabled={busy || !folderName.trim()} onClick={() => void createFolder()}>
                            新建
                          </button>
                          <button className="secondary-button" type="button" disabled={busy} onClick={() => { setCreatingFolder(false); setFolderName('') }}>
                            取消
                          </button>
                        </label>
                      ) : null}
                      {renameName ? (
                        <label>
                          <span>新名称</span>
                          <input
                            autoFocus
                            value={renameName}
                            onChange={(event) => setRenameName(event.target.value)}
                            onKeyDown={(event) => {
                              if (event.key === 'Enter' && !busy && renameName.trim()) {
                                event.preventDefault()
                                void renameEntry()
                              } else if (event.key === 'Escape') {
                                setRenameName('')
                              }
                            }}
                          />
                          <button className="primary-button" type="button" disabled={busy || !renameName.trim()} onClick={() => void renameEntry()}>
                            保存
                          </button>
                          <button className="secondary-button" type="button" disabled={busy} onClick={() => setRenameName('')}>
                            取消
                          </button>
                        </label>
                      ) : null}
                    </div>
                  ) : null}
                </>
              )}

              <div className="vault-file-list" role="list" aria-label={`${directory.repoName} 内容`}>
                {!loading && directory.entries.length > 0 ? (
                  <div className="vault-file-header" aria-hidden="true">
                    <span />
                    <span>名称</span>
                    <span className="file-cell--size">大小</span>
                    <span>修改时间</span>
                  </div>
                ) : null}
                {loading ? <p className="vault-muted vault-muted--padded">正在解密文件夹内容…</p> : null}
                {!loading && directory.skippedEntries > 0 ? (
                  <p className="vault-muted vault-muted--padded" role="status">
                    有 {directory.skippedEntries} 个项目无法解密，已隐藏。它们可能不是由此保险箱创建，或使用了其他 Safe Key。
                  </p>
                ) : null}
                {!loading && directory.entries.length === 0 && directory.skippedEntries === 0 ? (
                  <div className="vault-empty">
                    <span className="vault-empty__icon" aria-hidden="true">
                      <Folder size={20} strokeWidth={1.75} />
                    </span>
                    <strong>此加密文件夹为空</strong>
                  </div>
                ) : null}
                {directory.entries.map((entry) => {
                  const selected = selectedIds.has(entry.id)
                  const previewable = isPreviewableFile(entry)
                  return (
                    <button
                      className={`vault-file-row${selected ? ' vault-file-row--selected' : ''}`}
                      type="button"
                      role="listitem"
                      key={entry.id}
                      onClick={(event) => {
                        if (entry.entryType === 'dir' && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
                          void openDirectory(directory.repoId, entry.id)
                          return
                        }
                        if (event.ctrlKey || event.metaKey) {
                          setSelectedIds((current) => {
                            const next = new Set(current)
                            if (next.has(entry.id)) next.delete(entry.id)
                            else next.add(entry.id)
                            return next
                          })
                        } else {
                          setSelectedIds(new Set([entry.id]))
                        }
                      }}
                      onDoubleClick={entry.entryType === 'dir' ? undefined : () => {
                        if (previewable) {
                          onPreview(directory.repoId, entry, () => void downloadEntry(entry))
                        } else {
                          void downloadEntry(entry)
                        }
                      }}
                    >
                      <FileTypeIcon kind={entry.entryType === 'dir' ? 'folder' : fileKindByName(entry.name)} />
                      <span className="vault-file-row__name" translate="no">{entry.name}</span>
                      <span className="file-cell--size">{entry.entryType === 'dir' ? '文件夹' : formatBytes(entry.size)}</span>
                      <span>{formatModified(entry.modified)}</span>
                    </button>
                  )
                })}
              </div>
            </>
          )}

          {busy ? <p className="vault-muted">正在处理…</p> : null}
        </div>
      </Modal>

      {keyPrompt ? (
        <VaultSafeKeyModal
          prompt={keyPrompt}
          submitting={keyPromptBusy}
          error={keyPromptError}
          onClose={closeKeyPrompt}
          onSubmit={submitKeyPrompt}
        />
      ) : null}
    </>
  )
}
