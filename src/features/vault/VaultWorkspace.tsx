import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Check,
  ChevronRight,
  Copy,
  Download,
  Eye,
  Folder,
  FolderInput,
  FolderPlus,
  KeyRound,
  LoaderCircle,
  Lock,
  LockOpen,
  Pencil,
  RefreshCw,
  Search,
  ShieldCheck,
  ShieldPlus,
  Trash2,
  Upload,
} from 'lucide-react'
import { useDeferredValue, useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from 'react'
import { FileTypeIcon } from '../../components/FileTypeIcon'
import { Modal } from '../../components/Modal'
import { isPreviewableFile } from '../../preview'
import { commandErrorMessage, isCommandErrorCode, koofr } from '../../services/koofr'
import type { KoofrMount, RemoteFile, VaultDirectory, VaultEntry, VaultSummary } from '../../types/backend'
import type { VaultTransferSpec } from '../../types/vault'
import { fileKindByName, formatBytes, formatModified } from '../../utils/format'
import { VaultSafeKeyModal } from './VaultSafeKeyModal'
import type { VaultSafeKeyPrompt } from './VaultSafeKeyModal'

interface VaultWorkspaceProps {
  readonly mounts: readonly KoofrMount[]
  readonly defaultMountId: string
  readonly defaultParentPath: string
  readonly onNotice: (message: string) => void
  readonly onTransferStarted: (transfer: VaultTransferSpec) => void
  readonly onPreview: (repoId: string, entry: VaultEntry, onDownload: () => void) => void
}

interface Relocation {
  readonly entry: VaultEntry
  readonly isMove: boolean
}

type VaultSortKey = 'name' | 'modified' | 'size'
type VaultSortDir = 'asc' | 'desc'

function parentDirectory(path: string) {
  return path === '/' ? '/' : path.split('/').slice(0, -1).join('/') || '/'
}

export function VaultWorkspace({
  mounts,
  defaultMountId,
  defaultParentPath,
  onNotice,
  onTransferStarted,
  onPreview,
}: VaultWorkspaceProps) {
  const [vaults, setVaults] = useState<VaultSummary[]>([])
  const [directory, setDirectory] = useState<VaultDirectory | null>(null)
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [anchorId, setAnchorId] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [sortKey, setSortKey] = useState<VaultSortKey>('name')
  const [sortDir, setSortDir] = useState<VaultSortDir>('asc')
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  // Modals for vault creation & file actions
  const [createVaultOpen, setCreateVaultOpen] = useState(false)
  const [createName, setCreateName] = useState('')
  const [createMountId, setCreateMountId] = useState(defaultMountId)
  const [createParentPath, setCreateParentPath] = useState(defaultParentPath || '/')
  const [remoteBrowserOpen, setRemoteBrowserOpen] = useState(false)
  const [remoteBrowserPath, setRemoteBrowserPath] = useState('/')
  const [remoteBrowserBusy, setRemoteBrowserBusy] = useState(false)
  const [remoteFolders, setRemoteFolders] = useState<RemoteFile[]>([])
  const [createModalError, setCreateModalError] = useState('')

  const [createFolderOpen, setCreateFolderOpen] = useState(false)
  const [folderName, setFolderName] = useState('')
  const [renameTarget, setRenameTarget] = useState<VaultEntry | null>(null)
  const [renameName, setRenameName] = useState('')
  const [confirmingDelete, setConfirmingDelete] = useState(false)
  const [relocation, setRelocation] = useState<Relocation | null>(null)
  const [keyPrompt, setKeyPrompt] = useState<VaultSafeKeyPrompt | null>(null)
  const [keyPromptMountId, setKeyPromptMountId] = useState(defaultMountId)
  const [keyPromptError, setKeyPromptError] = useState('')
  const [keyPromptBusy, setKeyPromptBusy] = useState(false)

  const directoryRef = useRef<VaultDirectory | null>(null)
  const deferredQuery = useDeferredValue(query)

  const onlineMounts = useMemo(
    () => mounts.filter((mount) => mount.online),
    [mounts],
  )

  useEffect(() => {
    directoryRef.current = directory
  }, [directory])

  useEffect(() => {
    if (!createMountId || !onlineMounts.some((mount) => mount.id === createMountId)) {
      const preferred = onlineMounts.find((mount) => mount.id === defaultMountId)
        ?? onlineMounts.find((mount) => mount.isPrimary)
        ?? onlineMounts[0]
      if (preferred) setCreateMountId(preferred.id)
    }
  }, [createMountId, defaultMountId, onlineMounts])

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
      setAnchorId(null)
      setQuery('')
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

  const openCreateVaultModal = () => {
    const preferredMount = onlineMounts.find((mount) => mount.id === defaultMountId)
      ?? onlineMounts.find((mount) => mount.isPrimary)
      ?? onlineMounts[0]
    setCreateName('')
    setCreateMountId(preferredMount?.id ?? defaultMountId)
    setCreateParentPath(defaultParentPath || '/')
    setRemoteBrowserOpen(false)
    setCreateModalError('')
    setCreateVaultOpen(true)
  }

  const browseCreateRemote = async (path: string) => {
    if (!createMountId) return
    setRemoteBrowserBusy(true)
    setCreateModalError('')
    try {
      const files = await koofr.listFiles(createMountId, path)
      setRemoteFolders(files.filter((file) => file.entryType.toLowerCase() === 'dir'))
      setRemoteBrowserPath(path)
      setRemoteBrowserOpen(true)
    } catch (cause) {
      setCreateModalError(commandErrorMessage(cause, '无法读取 Koofr 文件夹'))
    } finally {
      setRemoteBrowserBusy(false)
    }
  }

  const openKeyPrompt = (prompt: VaultSafeKeyPrompt, targetMountId = defaultMountId) => {
    setError('')
    setKeyPromptError('')
    setKeyPromptMountId(targetMountId)
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
        const created = await koofr.createVault(
          keyPromptMountId || defaultMountId,
          keyPrompt.parentPath,
          keyPrompt.name,
          safeKey,
        )
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
    if (directory?.repoId === repoId) setDirectory(null)
    setRelocation(null)
  }, '无法锁定保险箱。')

  const proceedToCreateSafeKey = () => {
    const name = createName.trim()
    const targetMount = createMountId || defaultMountId
    if (!targetMount || !name) return
    setCreateVaultOpen(false)
    setRemoteBrowserOpen(false)
    openKeyPrompt({ kind: 'create', name, parentPath: createParentPath.trim() || '/' }, targetMount)
  }

  const importVault = () => run(async () => {
    setVaults(await koofr.importVaultRcloneConfig())
    onNotice('保险箱已导入。')
  }, '无法导入保险箱配置。')

  const refreshDirectory = () => {
    if (directory) void openDirectory(directory.repoId, directory.directoryId)
    else void loadVaults()
  }

  const toggleSort = (key: VaultSortKey) => {
    if (sortKey === key) {
      setSortDir((current) => (current === 'asc' ? 'desc' : 'asc'))
    } else {
      setSortKey(key)
      setSortDir(key === 'name' ? 'asc' : 'desc')
    }
  }

  const visibleEntries = useMemo(() => {
    const raw = directory?.entries ?? []
    const normalized = deferredQuery.trim().toLocaleLowerCase('zh-CN')
    const matched = normalized
      ? raw.filter((entry) => entry.name.toLocaleLowerCase('zh-CN').includes(normalized))
      : raw
    const direction = sortDir === 'asc' ? 1 : -1
    return [...matched].sort((left, right) => {
      const dirOrder = Number(right.entryType === 'dir') - Number(left.entryType === 'dir')
      if (dirOrder !== 0) return dirOrder
      if (sortKey === 'modified') {
        const diff = (left.modified - right.modified) * direction
        if (diff !== 0) return diff
      } else if (sortKey === 'size') {
        const diff = ((left.size ?? 0) - (right.size ?? 0)) * direction
        if (diff !== 0) return diff
      }
      return left.name.localeCompare(right.name, 'zh-CN', { numeric: true }) * direction
    })
  }, [deferredQuery, directory?.entries, sortDir, sortKey])

  const selected = useMemo(
    () => directory?.entries.filter((entry) => selectedIds.has(entry.id)) ?? [],
    [directory?.entries, selectedIds],
  )
  const singleSelected = selected.length === 1 ? selected[0] : null
  const previewableSelected = singleSelected && isPreviewableFile(singleSelected)
    ? singleSelected
    : null
  const allVisibleSelected = visibleEntries.length > 0
    && visibleEntries.every((entry) => selectedIds.has(entry.id))

  const toggleSelection = (id: string) => {
    setSelectedIds((current) => {
      const next = new Set(current)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
    setAnchorId(id)
  }

  const selectRow = (id: string, event: ReactMouseEvent) => {
    if (event.shiftKey && anchorId) {
      const start = visibleEntries.findIndex((entry) => entry.id === anchorId)
      const end = visibleEntries.findIndex((entry) => entry.id === id)
      if (start !== -1 && end !== -1) {
        const [from, to] = start <= end ? [start, end] : [end, start]
        const next = new Set(event.ctrlKey || event.metaKey ? selectedIds : [])
        for (let i = from; i <= to; i += 1) next.add(visibleEntries[i].id)
        setSelectedIds(next)
        return
      }
    }
    if (event.ctrlKey || event.metaKey) {
      toggleSelection(id)
      return
    }
    setSelectedIds(new Set([id]))
    setAnchorId(id)
  }

  const toggleAll = () => {
    setSelectedIds((current) => {
      const next = new Set(current)
      visibleEntries.forEach((entry) => {
        if (allVisibleSelected) next.delete(entry.id)
        else next.add(entry.id)
      })
      return next
    })
    setAnchorId(null)
  }

  const createFolder = () => run(async () => {
    if (!directory || !folderName.trim()) return
    await koofr.createVaultFolder(directory.repoId, directory.directoryId, folderName.trim())
    setFolderName('')
    setCreateFolderOpen(false)
    await openDirectory(directory.repoId, directory.directoryId)
  }, '无法创建加密文件夹。')

  const renameEntry = () => run(async () => {
    if (!directory || !renameTarget || !renameName.trim()) return
    await koofr.renameVaultEntry(directory.repoId, renameTarget.id, renameName.trim())
    setRenameTarget(null)
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

  const currentFolderTitle = directory?.breadcrumbs.at(-1)?.name || directory?.repoName || '私人保险箱'
  const parentBreadcrumb = directory && directory.breadcrumbs.length > 1
    ? directory.breadcrumbs[directory.breadcrumbs.length - 2]
    : null

  return (
    <main className="workspace vault-page" aria-label="私人保险箱">
      {!directory ? (
        <>
          <div className="workspace__header">
            <div>
              <h1>私人保险箱</h1>
              <p className="workspace__description">
                端到端加密空间。文件名与内容均在本地加解密，兼容官方 Koofr Vault 与 rclone。
              </p>
            </div>
            <div className="top-actions">
              <button className="secondary-button" type="button" disabled={busy} onClick={() => void importVault()}>
                <FolderInput size={15} strokeWidth={1.8} />
                导入 rclone 配置
              </button>
              <button
                className="icon-button icon-button--bordered"
                type="button"
                aria-label="刷新保险箱列表"
                title="刷新"
                disabled={busy || loading}
                onClick={() => void loadVaults()}
              >
                <RefreshCw className={loading ? 'icon-spin' : ''} size={15} strokeWidth={1.8} />
              </button>
              <button
                className="primary-button"
                type="button"
                disabled={busy || (!defaultMountId && onlineMounts.length === 0)}
                onClick={openCreateVaultModal}
              >
                <ShieldPlus size={15} strokeWidth={1.85} />
                新建保险箱
              </button>
            </div>
          </div>

          <div className="vault-page__body">
            {error ? <div className="vault-error" role="alert">{error}</div> : null}

            <div className="vault-section-header">
              <h2>我的保险箱</h2>
              {!loading ? <span className="vault-count">{vaults.length} 个</span> : null}
            </div>

            {loading ? (
              <div className="empty-state">
                <RefreshCw className="icon-spin" size={20} strokeWidth={1.7} />
                <strong>正在读取保险箱…</strong>
                <span>请稍候</span>
              </div>
            ) : vaults.length === 0 ? (
              <div className="empty-state">
                <span className="sync-empty__icon" aria-hidden="true">
                  <ShieldCheck size={24} strokeWidth={1.7} />
                </span>
                <h2>还没有保险箱</h2>
                <p>新建一个端到端加密保险箱，或导入已有的 rclone crypt 配置文件。</p>
                <div className="vault-empty-actions">
                  <button
                    className="primary-button"
                    type="button"
                    disabled={busy || (!defaultMountId && onlineMounts.length === 0)}
                    onClick={openCreateVaultModal}
                  >
                    <ShieldPlus size={15} strokeWidth={1.85} />
                    新建保险箱
                  </button>
                  <button className="secondary-button" type="button" disabled={busy} onClick={() => void importVault()}>
                    <FolderInput size={15} strokeWidth={1.8} />
                    导入 rclone 配置
                  </button>
                </div>
              </div>
            ) : (
              <div className="vault-list">
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
                            : `已解锁 · ${Math.round(vault.autoLockSeconds / 60)} 分钟无操作后自动锁定`}
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
                          <KeyRound size={14} strokeWidth={1.8} />
                          解锁
                        </button>
                      ) : (
                        <>
                          <button
                            className="primary-button"
                            type="button"
                            disabled={busy}
                            onClick={() => void openDirectory(vault.id, 'root')}
                          >
                            <Folder size={14} strokeWidth={1.8} />
                            打开
                          </button>
                          <button
                            className="secondary-button"
                            type="button"
                            disabled={busy}
                            onClick={() => void lock(vault.id)}
                          >
                            <Lock size={14} strokeWidth={1.8} />
                            锁定
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
            )}
          </div>
        </>
      ) : (
        <>
          <div className="workspace__topbar workspace__topbar--vault">
            <button
              className="secondary-button"
              type="button"
              disabled={busy}
              onClick={() => {
                setDirectory(null)
                setRelocation(null)
                setSelectedIds(new Set())
              }}
            >
              <ArrowLeft size={14} strokeWidth={1.85} />
              保险箱列表
            </button>
            <nav className="breadcrumb" aria-label="保险箱路径">
              {directory.breadcrumbs.map((crumb, index) => (
                <span className="breadcrumb__segment" key={crumb.id}>
                  {index > 0 ? <span>/</span> : null}
                  <button
                    translate="no"
                    type="button"
                    disabled={busy || crumb.id === directory.directoryId}
                    onClick={() => void openDirectory(directory.repoId, crumb.id)}
                  >
                    {crumb.name}
                  </button>
                </span>
              ))}
            </nav>
            <label className="search-box">
              <Search size={15} strokeWidth={1.8} />
              <input
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="筛选当前加密文件夹"
                aria-label="筛选当前加密文件夹"
              />
            </label>
            <div className="top-actions">
              <button className="primary-button" type="button" disabled={busy} onClick={() => void upload()}>
                <Upload size={15} strokeWidth={1.85} />
                加密上传
              </button>
              <button
                className="secondary-button"
                type="button"
                disabled={busy}
                onClick={() => {
                  setFolderName('')
                  setCreateFolderOpen(true)
                }}
              >
                <FolderPlus size={15} strokeWidth={1.8} />
                新建文件夹
              </button>
              <button
                className="icon-button icon-button--bordered"
                type="button"
                aria-label="刷新"
                title="刷新"
                disabled={busy || loading}
                onClick={refreshDirectory}
              >
                <RefreshCw className={loading ? 'icon-spin' : ''} size={15} strokeWidth={1.8} />
              </button>
              <button
                className="secondary-button"
                type="button"
                disabled={busy}
                onClick={() => void lock(directory.repoId)}
              >
                <Lock size={14} strokeWidth={1.8} />
                锁定
              </button>
            </div>
          </div>

          {error ? <div className="vault-page__banner"><div className="vault-error" role="alert">{error}</div></div> : null}

          {relocation ? (
            <div className="vault-page__banner">
              <div className="vault-relocation">
                <span>选择目标文件夹：<bdi translate="no">{relocation.entry.name}</bdi></span>
                <button className="primary-button" type="button" disabled={busy} onClick={() => void completeRelocation()}>
                  {relocation.isMove ? <FolderInput size={14} strokeWidth={1.8} /> : <Copy size={14} strokeWidth={1.8} />}
                  {relocation.isMove ? '移动到此处' : '复制到此处'}
                </button>
                <button className="secondary-button" type="button" onClick={() => setRelocation(null)}>取消</button>
              </div>
            </div>
          ) : null}

          <div className="workspace__heading-row">
            <header className="workspace__heading">
              <h1 translate="no">{currentFolderTitle}</h1>
              <p>{directory.entries.length} 个项目 · {loading ? '正在解密…' : '端到端加密'}</p>
            </header>

            {selected.length > 0 && !relocation ? (
              <div className="selection-toolbar" aria-label="所选加密项目操作">
                <strong>{selected.length}</strong>
                <span>已选中</span>
                <i />
                <button
                  type="button"
                  disabled={busy || !singleSelected || singleSelected.entryType !== 'file'}
                  onClick={() => singleSelected && void downloadEntry(singleSelected)}
                >
                  <Download size={15} strokeWidth={1.8} />
                  解密下载
                </button>
                <button
                  type="button"
                  disabled={busy || !previewableSelected}
                  onClick={() => {
                    if (previewableSelected) {
                      onPreview(directory.repoId, previewableSelected, () => void downloadEntry(previewableSelected))
                    }
                  }}
                >
                  <Eye size={15} strokeWidth={1.8} />
                  预览
                </button>
                <button
                  type="button"
                  disabled={busy || !singleSelected}
                  onClick={() => {
                    if (singleSelected) {
                      setRenameTarget(singleSelected)
                      setRenameName(singleSelected.name)
                    }
                  }}
                >
                  <Pencil size={15} strokeWidth={1.8} />
                  重命名
                </button>
                <button
                  type="button"
                  disabled={busy || !singleSelected}
                  onClick={() => singleSelected && setRelocation({ entry: singleSelected, isMove: true })}
                >
                  <FolderInput size={15} strokeWidth={1.8} />
                  移动
                </button>
                <button
                  type="button"
                  disabled={busy || !singleSelected}
                  onClick={() => singleSelected && setRelocation({ entry: singleSelected, isMove: false })}
                >
                  <Copy size={15} strokeWidth={1.8} />
                  复制
                </button>
                <button
                  className="selection-action--danger"
                  type="button"
                  disabled={busy}
                  onClick={() => setConfirmingDelete(true)}
                >
                  <Trash2 size={15} strokeWidth={1.8} />
                  删除
                </button>
              </div>
            ) : null}
          </div>

          <section
            className={`file-list${selected.length > 0 ? ' file-list--has-selection' : ''}`}
            role="grid"
            aria-label={`${directory.repoName} 内容`}
            aria-busy={loading}
          >
            <div className="file-row file-row--header" role="row">
              <button
                className={`checkbox${allVisibleSelected ? ' checkbox--checked' : ''}`}
                type="button"
                role="checkbox"
                aria-checked={allVisibleSelected}
                onClick={toggleAll}
                aria-label="全选当前文件"
              >
                {allVisibleSelected ? <Check size={12} strokeWidth={2.4} /> : null}
              </button>
              <span role="columnheader" aria-sort={sortKey === 'name' ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}>
                <button
                  className={`column-sort${sortKey === 'name' ? ' column-sort--active' : ''}`}
                  type="button"
                  onClick={() => toggleSort('name')}
                >
                  名称
                  {sortKey === 'name' ? (sortDir === 'asc' ? <ArrowUp size={12} aria-hidden="true" /> : <ArrowDown size={12} aria-hidden="true" />) : null}
                </button>
              </span>
              <span role="columnheader" aria-sort={sortKey === 'modified' ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}>
                <button
                  className={`column-sort${sortKey === 'modified' ? ' column-sort--active' : ''}`}
                  type="button"
                  onClick={() => toggleSort('modified')}
                >
                  修改时间
                  {sortKey === 'modified' ? (sortDir === 'asc' ? <ArrowUp size={12} aria-hidden="true" /> : <ArrowDown size={12} aria-hidden="true" />) : null}
                </button>
              </span>
              <span className="file-cell--size" role="columnheader" aria-sort={sortKey === 'size' ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}>
                <button
                  className={`column-sort${sortKey === 'size' ? ' column-sort--active' : ''}`}
                  type="button"
                  onClick={() => toggleSort('size')}
                >
                  大小
                  {sortKey === 'size' ? (sortDir === 'asc' ? <ArrowUp size={12} aria-hidden="true" /> : <ArrowDown size={12} aria-hidden="true" />) : null}
                </button>
              </span>
              <span role="columnheader" aria-hidden="true" />
            </div>

            {parentBreadcrumb ? (
              <div className="file-row file-row--parent" role="row">
                <span role="gridcell" aria-hidden="true" />
                <button
                  className="file-name file-name--button"
                  type="button"
                  onClick={() => void openDirectory(directory.repoId, parentBreadcrumb.id)}
                >
                  <FileTypeIcon kind="folder" />
                  <strong>..</strong>
                </button>
                <span role="gridcell">返回上一级目录</span>
                <span className="file-cell--size" role="gridcell" />
                <button
                  className="row-action"
                  type="button"
                  aria-label="返回上一级目录"
                  onClick={() => void openDirectory(directory.repoId, parentBreadcrumb.id)}
                >
                  <ArrowUp size={15} strokeWidth={1.8} />
                </button>
              </div>
            ) : null}

            {!loading && directory.skippedEntries > 0 ? (
              <p className="vault-muted vault-muted--padded" role="status">
                有 {directory.skippedEntries} 个项目无法解密，已隐藏。它们可能不是由此保险箱创建，或使用了其他 Safe Key。
              </p>
            ) : null}

            {visibleEntries.map((entry) => {
              const isSelected = selectedIds.has(entry.id)
              const isDir = entry.entryType === 'dir'
              const previewable = isPreviewableFile(entry)
              return (
                <div
                  className={`file-row${isSelected ? ' file-row--selected' : ''}`}
                  role="row"
                  aria-selected={isSelected}
                  key={entry.id}
                >
                  <button
                    className={`checkbox${isSelected ? ' checkbox--checked' : ''}`}
                    type="button"
                    role="checkbox"
                    aria-checked={isSelected}
                    aria-label={`${isSelected ? '取消选择' : '选择'} ${entry.name}`}
                    onClick={() => toggleSelection(entry.id)}
                  >
                    {isSelected ? <Check size={12} strokeWidth={2.4} /> : null}
                  </button>
                  <button
                    className="file-name file-name--button"
                    type="button"
                    onClick={(event) => {
                      if (isDir && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
                        void openDirectory(directory.repoId, entry.id)
                        return
                      }
                      selectRow(entry.id, event)
                    }}
                    onDoubleClick={isDir ? undefined : () => {
                      if (previewable) {
                        onPreview(directory.repoId, entry, () => void downloadEntry(entry))
                      } else {
                        void downloadEntry(entry)
                      }
                    }}
                  >
                    <FileTypeIcon kind={isDir ? 'folder' : fileKindByName(entry.name)} />
                    <strong translate="no">{entry.name}</strong>
                  </button>
                  <span role="gridcell">{formatModified(entry.modified)}</span>
                  <span className="file-cell--size" role="gridcell">{isDir ? '-' : formatBytes(entry.size)}</span>
                  {previewable ? (
                    <div className="row-actions" role="gridcell">
                      <button
                        className="row-action"
                        type="button"
                        aria-label={`预览 ${entry.name}`}
                        onClick={() => onPreview(directory.repoId, entry, () => void downloadEntry(entry))}
                      >
                        <Eye size={15} strokeWidth={1.8} />
                      </button>
                      <button
                        className="row-action"
                        type="button"
                        aria-label={`下载 ${entry.name}`}
                        onClick={() => void downloadEntry(entry)}
                      >
                        <Download size={15} strokeWidth={1.8} />
                      </button>
                    </div>
                  ) : (
                    <button
                      className="row-action"
                      type="button"
                      aria-label={isDir ? `打开 ${entry.name}` : `下载 ${entry.name}`}
                      onClick={() => (isDir ? void openDirectory(directory.repoId, entry.id) : void downloadEntry(entry))}
                    >
                      {isDir ? <ChevronRight size={16} strokeWidth={1.8} /> : <Download size={15} strokeWidth={1.8} />}
                    </button>
                  )}
                </div>
              )
            })}

            {loading && visibleEntries.length === 0 ? (
              <div className="empty-state">
                <RefreshCw className="icon-spin" size={20} strokeWidth={1.7} />
                <strong>正在解密文件夹内容…</strong>
                <span>请稍候</span>
              </div>
            ) : null}

            {!loading && visibleEntries.length === 0 && directory.skippedEntries === 0 ? (
              <div className="empty-state">
                {query ? <Search size={20} strokeWidth={1.7} /> : <Folder size={20} strokeWidth={1.7} />}
                <strong>{query ? '没有匹配的项目' : '此加密文件夹为空'}</strong>
                <span>{query ? '试试筛选其他文件名' : '点击右上角加密上传文件或新建文件夹'}</span>
              </div>
            ) : null}
          </section>
        </>
      )}

      {createVaultOpen ? (
        <Modal
          title="新建保险箱"
          actionLabel="下一步"
          actionDisabled={busy || !createName.trim() || !(createMountId || defaultMountId)}
          secondaryActionLabel="取消"
          wide
          onClose={() => setCreateVaultOpen(false)}
          onSecondaryAction={() => setCreateVaultOpen(false)}
          onAction={proceedToCreateSafeKey}
        >
          <div className="vault-create-modal">
            {createModalError ? <div className="vault-error" role="alert">{createModalError}</div> : null}

            <label className="modal-field">
              <span>保险箱名称</span>
              <input
                autoFocus
                value={createName}
                maxLength={255}
                placeholder="输入新保险箱名称"
                onChange={(event) => setCreateName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !busy && createName.trim() && (createMountId || defaultMountId)) {
                    event.preventDefault()
                    proceedToCreateSafeKey()
                  }
                }}
              />
            </label>

            {onlineMounts.length > 0 ? (
              <div className="sync-wizard__row">
                <label className="sync-wizard__field sync-wizard__field--mount">
                  <span>存储位置</span>
                  <select
                    className="input"
                    value={createMountId}
                    aria-label="保险箱存储位置"
                    onChange={(event) => {
                      setCreateMountId(event.target.value)
                      setRemoteBrowserOpen(false)
                    }}
                  >
                    {onlineMounts.map((mount) => (
                      <option translate="no" key={mount.id} value={mount.id}>{mount.name}</option>
                    ))}
                  </select>
                </label>

                <div className="sync-wizard__field sync-wizard__field--path">
                  <label htmlFor="vault-parent-path">存放文件夹</label>
                  <div className="sync-remote-picker__row">
                    <input
                      id="vault-parent-path"
                      className="input"
                      value={createParentPath}
                      aria-label="存放文件夹"
                      onChange={(event) => setCreateParentPath(event.target.value)}
                    />
                    <button
                      className="button"
                      type="button"
                      disabled={remoteBrowserBusy || !createMountId}
                      onClick={() => {
                        if (remoteBrowserOpen) setRemoteBrowserOpen(false)
                        else void browseCreateRemote(createParentPath.trim() || '/')
                      }}
                    >
                      {remoteBrowserBusy ? <LoaderCircle className="spin" size={14} /> : '浏览 Koofr'}
                    </button>
                  </div>
                </div>
              </div>
            ) : (
              <div className="vault-create__header">
                <span>目标位置</span>
                <code className="vault-create__path" translate="no">{createParentPath || '/'}</code>
              </div>
            )}

            {remoteBrowserOpen ? (
              <div className="sync-remote-browser" role="region" aria-label="选择 Koofr 文件夹">
                <div className="sync-remote-picker__toolbar">
                  <button
                    className="sync-picker-action sync-picker-action--secondary"
                    type="button"
                    disabled={remoteBrowserBusy || remoteBrowserPath === '/'}
                    onClick={() => void browseCreateRemote(parentDirectory(remoteBrowserPath))}
                  >
                    <ArrowLeft size={14} />
                    返回上一级
                  </button>
                  <strong className="sync-remote-picker__path" translate="no" title={remoteBrowserPath}>
                    {remoteBrowserPath}
                  </strong>
                  <button
                    className="sync-picker-action sync-picker-action--primary"
                    type="button"
                    onClick={() => {
                      setCreateParentPath(remoteBrowserPath)
                      setRemoteBrowserOpen(false)
                    }}
                  >
                    <Check size={14} />
                    选择此目录
                  </button>
                </div>
                <div className="sync-remote-picker__divider" />
                <div className="sync-remote-browser__list">
                  {remoteFolders.map((folder) => (
                    <button
                      className="sync-remote-picker__item"
                      type="button"
                      key={folder.path}
                      onClick={() => void browseCreateRemote(folder.path)}
                    >
                      <Folder size={15} strokeWidth={1.8} aria-hidden="true" />
                      <span translate="no">{folder.name}</span>
                      <ChevronRight size={14} aria-hidden="true" />
                    </button>
                  ))}
                  {!remoteFolders.length ? (
                    <span className="sync-remote-picker__empty">当前目录没有子文件夹</span>
                  ) : null}
                </div>
              </div>
            ) : null}
          </div>
        </Modal>
      ) : null}

      {createFolderOpen ? (
        <Modal
          title="新建加密文件夹"
          actionLabel={busy ? '正在创建…' : '创建文件夹'}
          actionDisabled={busy || !folderName.trim()}
          secondaryActionLabel="取消"
          onClose={() => setCreateFolderOpen(false)}
          onSecondaryAction={() => setCreateFolderOpen(false)}
          onAction={() => void createFolder()}
        >
          <label className="modal-field">
            <span>文件夹名称</span>
            <input
              autoFocus
              value={folderName}
              maxLength={255}
              placeholder="文件夹名称"
              onChange={(event) => setFolderName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !busy && folderName.trim()) {
                  event.preventDefault()
                  void createFolder()
                }
              }}
            />
          </label>
        </Modal>
      ) : null}

      {renameTarget ? (
        <Modal
          title="重命名加密项目"
          actionLabel={busy ? '正在保存…' : '保存名称'}
          actionDisabled={busy || !renameName.trim() || renameName.trim() === renameTarget.name}
          secondaryActionLabel="取消"
          onClose={() => setRenameTarget(null)}
          onSecondaryAction={() => setRenameTarget(null)}
          onAction={() => void renameEntry()}
        >
          <label className="modal-field">
            <span>新名称</span>
            <input
              autoFocus
              value={renameName}
              maxLength={255}
              onChange={(event) => setRenameName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !busy && renameName.trim() && renameName.trim() !== renameTarget.name) {
                  event.preventDefault()
                  void renameEntry()
                }
              }}
            />
          </label>
        </Modal>
      ) : null}

      {confirmingDelete && selected.length > 0 ? (
        <Modal
          title="确认删除"
          actionLabel={busy ? '正在删除…' : `删除 ${selected.length} 个项目`}
          actionDisabled={busy}
          secondaryActionLabel="取消"
          onClose={() => setConfirmingDelete(false)}
          onSecondaryAction={() => setConfirmingDelete(false)}
          onAction={() => void deleteEntries()}
        >
          <p>确定删除所选 {selected.length} 个加密项目吗？</p>
        </Modal>
      ) : null}

      {keyPrompt ? (
        <VaultSafeKeyModal
          prompt={keyPrompt}
          submitting={keyPromptBusy}
          error={keyPromptError}
          onClose={closeKeyPrompt}
          onSubmit={submitKeyPrompt}
        />
      ) : null}
    </main>
  )
}
