import { useCallback, useEffect, useRef, useState } from 'react'
import { ArrowDownToLine } from 'lucide-react'
import { AppSidebar } from './components/AppSidebar'
import { Modal } from './components/Modal'
import { ThemePicker } from './components/ThemePicker'
import { TitleBar } from './components/TitleBar'
import { LoginPage } from './features/auth/LoginPage'
import { CollectionWorkspace } from './features/files/CollectionWorkspace'
import { FileWorkspace } from './features/files/FileWorkspace'
import { CreateShareLinkDialog } from './features/links/CreateShareLinkDialog'
import { FilePreviewModal } from './features/files/FilePreviewModal'
import { MoveDestinationDialog } from './features/files/MoveDestinationDialog'
import { isDirectory } from './features/files/filePresentation'
import { isCollectionView, useKoofrCollections } from './features/files/useKoofrCollections'
import { useKoofrWorkspace } from './features/files/useKoofrWorkspace'
import { SettingsPanel } from './features/settings/SettingsPanel'
import { DownloadDestinationDialog } from './features/transfers/DownloadDestinationDialog'
import { SplitUploadDialog } from './features/transfers/SplitUploadDialog'
import { TransferPanel } from './features/transfers/TransferPanel'
import { SyncWorkspace } from './features/sync/SyncWorkspace'
import { VaultDialog } from './features/vault/VaultDialog'
import { countActiveDownloads } from './features/transfers/transferProgress'
import { shouldPromptForFloatingWindow } from './features/transfers/floatingTransfer'
import {
  commandErrorMessage,
  isTauriRuntime,
  koofr,
} from './services/koofr'
import { readStoredTheme, storeTheme, type ThemeId } from './theme'
import { useAppSettings } from './hooks/useAppSettings'
import { useTransferManager } from './hooks/useTransferManager'
import { useSyncManager } from './hooks/useSyncManager'
import { installRuntimeTranslations } from './i18n'
import type {
  LocatedFile,
  LocalFileSelection,
  RemoteFile,
  TrashItem,
  FilePreviewInfo,
  VaultEntry,
} from './types/backend'
import type { SplitUploadSettings, UploadMode } from './types/files'

type ModalKind = 'settings' | 'theme' | 'vault' | 'download' | 'splitUpload' | 'floatingPrompt' | 'share' | 'createFolder' | 'rename' | 'move' | 'delete' | 'emptyTrash' | null
type AuthState = 'checking' | 'signedOut' | 'signingIn' | 'signedIn'

export interface PendingDownload {
  readonly file: RemoteFile
  readonly mountId: string
}

interface PendingSplitUpload {
  readonly selection: LocalFileSelection
  readonly mountId: string
  readonly path: string
}

interface FilePreviewState {
  readonly fileName: string
  readonly previewInfo: FilePreviewInfo
  readonly onDownload: () => void
}

function App() {
  const [activeItem, setActiveItem] = useState('我的文件')
  const [authState, setAuthState] = useState<AuthState>('checking')
  const [loginError, setLoginError] = useState('')
  const [savedEmail, setSavedEmail] = useState('')
  const [modalKind, setModalKind] = useState<ModalKind>(null)
  const [modalInput, setModalInput] = useState('')
  const [pendingFiles, setPendingFiles] = useState<RemoteFile[]>([])
  const [operationBusy, setOperationBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const [themeId, setThemeId] = useState<ThemeId>(readStoredTheme)

  const noticeTimeoutRef = useRef<number | null>(null)
  const showNotice = useCallback((message: string) => {
    setNotice(message)
    if (noticeTimeoutRef.current !== null) {
      window.clearTimeout(noticeTimeoutRef.current)
    }
    noticeTimeoutRef.current = window.setTimeout(() => {
      setNotice('')
      noticeTimeoutRef.current = null
    }, 3200)
  }, [])

  const onEmailCleared = useCallback(() => setSavedEmail(''), [])
  const {
    settings,
    setSettings,
    settingsLoading,
    settingsBusy,
    settingsError,
    setSettingsError,
    downloadSettingsError,
    versionInfo,
    openSettings,
    updateCacheSettings,
    updateLoggingSettings,
    updateTransferSettings,
    browseWorkDirectory,
    updateWorkDirectory,
    updateDownloadSettings,
    browseSettingsDownloadDirectory,
    clearMetadataCache,
    clearLogs,
    forgetSavedLogin,
    updateAppearance,
    updateGeneral,
    updateAdvancedTransfer,
    updateDownloadBehavior,
    updateBandwidth,
    updateNotifications,
    answerFloatingWindowPrompt,
    updateLinkDefaults,
    updateSecurity,
    updateHistoryPolicy,
  } = useAppSettings(showNotice, onEmailCleared)

  const [pendingDownload, setPendingDownload] = useState<PendingDownload | null>(null)
  const [pendingSplitUpload, setPendingSplitUpload] = useState<PendingSplitUpload | null>(null)
  const [filePreview, setFilePreview] = useState<FilePreviewState | null>(null)
  const workspace = useKoofrWorkspace(authState === 'signedIn')
  const collections = useKoofrCollections(authState === 'signedIn', activeItem)
  const activeMount = workspace.mounts.find((mount) => mount.id === workspace.activeMountId)
  const sync = useSyncManager(authState, showNotice)

  const openDownloadDestination = useCallback((
    file: RemoteFile,
    mountId: string,
  ) => {
    setPendingDownload({ file, mountId })
    setModalKind('download')
  }, [])

  const {
    transfers,
    setTransfers,
    transferVisible,
    setTransferVisible,
    downloadBusy,
    setDownloadBusy,
    downloadError,
    setDownloadError,
    downloadDirectory,
    startDownload,
    handleDownload,
    browseDownloadDirectory,
    cancelTransfer,
    pauseTransfer,
    resumeTransfer,
    discardResumableTransfer,
    clearFinishedTransfers,
    openDownloadedFile,
    openDownloadedFolder,
    startVaultTransfer,
    startSelectedUpload,
  } = useTransferManager(
    authState,
    settings,
    themeId,
    workspace.refreshCurrent,
    workspace.refreshIfAt,
    showNotice,
    openDownloadDestination,
    sync,
  )

  const confirmDownload = useCallback(async (targetDirectory: string, askEveryTime: boolean) => {
    if (!pendingDownload) return
    setDownloadBusy(true)
    setDownloadError('')
    try {
      if (!askEveryTime || settings?.askDownloadLocation !== askEveryTime) {
        setSettings(await koofr.updateDownloadSettings(targetDirectory, askEveryTime))
      }
      await startDownload(pendingDownload.file, pendingDownload.mountId, targetDirectory)
      setModalKind(null)
      setPendingDownload(null)
    } catch (error) {
      setDownloadError(commandErrorMessage(error, '无法使用这个下载位置，请检查路径后重试。'))
    } finally {
      setDownloadBusy(false)
    }
  }, [pendingDownload, setDownloadBusy, setDownloadError, setSettings, settings?.askDownloadLocation, startDownload])
  const activeDownloadCount = countActiveDownloads(transfers)
  const activeTransferCount = transfers.filter((item) =>
    item.state === 'queued' || item.state === 'running' || item.state === 'retrying').length
  // Read by the idle-logout timer without restarting it on every progress event.
  const activeTransferCountRef = useRef(0)

  useEffect(() => {
    activeTransferCountRef.current = activeTransferCount
  }, [activeTransferCount])

  useEffect(() => {
    const root = document.querySelector<HTMLElement>('.app-shell')
    if (!root) return
    return installRuntimeTranslations(root, settings?.language ?? 'system')
  }, [settings?.language, authState, modalKind])

  useEffect(() => {
    let active = true

    if (!isTauriRuntime()) {
      setAuthState('signedOut')
      return () => { active = false }
    }

    void koofr.getSettings()
      .then((loadedSettings) => {
        if (active) setSettings(loadedSettings)
      })
      .catch((error) => {
        if (active) {
          setSettingsError(commandErrorMessage(error, '无法加载应用设置。'))
        }
      })

    koofr.restoreSavedLogin()
      .then((bootstrap) => {
        if (!active) return
        setSavedEmail(bootstrap.savedEmail ?? '')
        setAuthState(bootstrap.session.authenticated ? 'signedIn' : 'signedOut')
      })
      .catch(async (error) => {
        if (active) {
          setLoginError(commandErrorMessage(error, '无法恢复保存的登录信息，请手动登录。'))
          try {
            const localSettings = await koofr.getSettings()
            if (active) setSavedEmail(localSettings.savedEmail ?? '')
          } catch {
            // The login form remains usable even if optional local settings cannot be read.
          }
          setAuthState('signedOut')
        }
      })

    return () => { active = false }
  }, [])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let disposed = false
    let unlisten: (() => void) | undefined
    void koofr.onFloatingWindowDisabled(() => {
      setSettings((current) => current
        ? {
            ...current,
            floatingWindowEnabled: false,
            floatingWindowPrompted: true,
          }
        : current)
    }).then((stopListening) => {
      if (disposed) stopListening()
      else unlisten = stopListening
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [setSettings])

  // Set once the floating window prompt was answered in this session, even if
  // saving the answer failed, so the prompt is not reopened right away.
  const floatingPromptAnsweredRef = useRef(false)

  useEffect(() => {
    if (shouldPromptForFloatingWindow(
      authState === 'signedIn',
      settings !== null,
      (settings?.floatingWindowPrompted ?? false) || floatingPromptAnsweredRef.current,
      modalKind !== null,
    )) {
      setModalKind('floatingPrompt')
    }
  }, [authState, modalKind, settings])

  const respondToFloatingPrompt = useCallback(
    async (enabled: boolean) => {
      floatingPromptAnsweredRef.current = true
      await answerFloatingWindowPrompt(enabled)
      // Close whatever the outcome: a failure is reported as a notice, and keeping
      // the dialog open would leave the whole window unusable.
      setModalKind((current) => (current === 'floatingPrompt' ? null : current))
    },
    [answerFloatingWindowPrompt],
  )

  useEffect(() => {
    if (authState !== 'signedIn') return
    if (!settings?.confirmQuitWithTransfers) return
    const hasActive = transfers.some(
      (item) =>
        item.state === 'queued' || item.state === 'running' || item.state === 'retrying',
    )
    if (!hasActive) return
    const handler = (event: BeforeUnloadEvent) => {
      event.preventDefault()
      event.returnValue = ''
    }
    window.addEventListener('beforeunload', handler)
    return () => window.removeEventListener('beforeunload', handler)
  }, [authState, settings?.confirmQuitWithTransfers, transfers])

  useEffect(() => {
    if (authState !== 'signedIn') return
    const minutes = settings?.idleAutoLogoutMinutes ?? 0
    if (!minutes) return
    const timeout = minutes * 60 * 1000
    let lastActive = Date.now()
    const markActive = () => {
      lastActive = Date.now()
    }
    const events: Array<keyof WindowEventMap> = [
      'mousemove',
      'mousedown',
      'keydown',
      'touchstart',
      'wheel',
    ]
    events.forEach((event) => window.addEventListener(event, markActive, { passive: true }))
    const interval = window.setInterval(() => {
      // A running transfer counts as activity: signing out would interrupt it,
      // so the idle countdown only starts once the queue is empty.
      if (activeTransferCountRef.current > 0) {
        lastActive = Date.now()
        return
      }
      if (Date.now() - lastActive >= timeout && authState === 'signedIn') {
        void logout()
      }
    }, Math.max(30_000, Math.min(timeout / 2, 5 * 60_000)))
    return () => {
      window.clearInterval(interval)
      events.forEach((event) => window.removeEventListener(event, markActive))
    }
  }, [authState, settings?.idleAutoLogoutMinutes])

  const selectTheme = (nextTheme: ThemeId) => {
    setThemeId(nextTheme)
    storeTheme(nextTheme)
  }

  const login = async (email: string, appPassword: string, rememberPassword: boolean) => {
    if (!isTauriRuntime()) {
      setLoginError('登录功能需要在 Koofr 桌面应用中使用。')
      return
    }

    setLoginError('')
    setAuthState('signingIn')
    try {
      const session = await koofr.connect(email, appPassword, rememberPassword)
      if (!session.authenticated) {
        setLoginError('Koofr 未能建立登录会话，请重试。')
        setAuthState('signedOut')
        return
      }
      setSavedEmail(rememberPassword ? email : '')
      setAuthState('signedIn')
    } catch (error) {
      setLoginError(commandErrorMessage(error, '登录失败，请稍后重试。'))
      setAuthState('signedOut')
    }
  }

  const logout = useCallback(async () => {
    try {
      await koofr.disconnect()
      setModalKind(null)
      setFilePreview(null)
      setLoginError('')
      setTransfers([])
      setAuthState('signedOut')
    } catch (error) {
      showNotice(commandErrorMessage(error, '暂时无法退出登录，请重试。'))
    }
  }, [setTransfers, showNotice])





  const handleUpload = useCallback(async (mode: UploadMode) => {
    if (!workspace.activeMountId) return
    const uploadMountId = workspace.activeMountId
    const uploadPath = workspace.path
    try {
      const selection = await koofr.selectUploadFile()
      if (!selection) return
      if (mode === 'split') {
        setPendingSplitUpload({ selection, mountId: uploadMountId, path: uploadPath })
        setModalKind('splitUpload')
        return
      }
      const thresholdBytes = (settings?.autoSplitThresholdMib ?? 512) * 1024 * 1024
      if (
        selection.fileSize !== null
        && selection.fileSize > thresholdBytes
        && window.confirm(
          `文件超过 ${settings?.autoSplitThresholdMib ?? 512} MiB。选择“确定”改用分卷上传；选择“取消”继续普通上传。`,
        )
      ) {
        setPendingSplitUpload({ selection, mountId: uploadMountId, path: uploadPath })
        setModalKind('splitUpload')
        return
      }
      await startSelectedUpload(selection, uploadMountId, uploadPath, null)
    } catch (error) {
      showNotice(commandErrorMessage(error, '无法选择本地文件。'))
    }
  }, [settings?.autoSplitThresholdMib, showNotice, startSelectedUpload, workspace.activeMountId, workspace.path])

  const confirmSplitUpload = useCallback((splitSettings: SplitUploadSettings) => {
    if (!pendingSplitUpload) return
    const pending = pendingSplitUpload
    setPendingSplitUpload(null)
    setModalKind(null)
    void startSelectedUpload(pending.selection, pending.mountId, pending.path, splitSettings)
  }, [pendingSplitUpload, startSelectedUpload])



  const openFilePreview = async (
    file: RemoteFile,
    mountId: string,
    onDownload: () => void,
  ) => {
    if (!mountId) return
    try {
      const previewInfo = await koofr.prepareFilePreview(mountId, file.path)
      setFilePreview({ fileName: file.name, previewInfo, onDownload })
    } catch (error) {
      showNotice(commandErrorMessage(error, '无法预览此文件，请下载后打开。'))
    }
  }

  const openVaultFilePreview = async (
    repoId: string,
    entry: VaultEntry,
    onDownload: () => void,
  ) => {
    try {
      const previewInfo = await koofr.prepareVaultPreview(repoId, entry.id)
      setFilePreview({ fileName: entry.name, previewInfo, onDownload })
    } catch (error) {
      showNotice(commandErrorMessage(error, '无法预览此加密文件，请下载后打开。'))
    }
  }

  // Stable while a preview is open, so unrelated app re-renders do not hand the
  // modal new callbacks.
  const closeFilePreview = useCallback(() => {
    if (filePreview) {
      koofr.closeFilePreview(filePreview.previewInfo.sessionId).catch(() => {})
    }
    setFilePreview(null)
  }, [filePreview])

  const downloadPreviewedFile = useCallback(() => {
    if (!filePreview) return
    const download = filePreview.onDownload
    closeFilePreview()
    download()
  }, [closeFilePreview, filePreview])

  const openCreateFolder = () => {
    setModalInput('')
    setPendingFiles([])
    setModalKind('createFolder')
  }

  const createFolder = async () => {
    const name = modalInput.trim()
    if (!workspace.activeMountId || !name) return
    setOperationBusy(true)
    try {
      await koofr.createFolder(workspace.activeMountId, workspace.path, name)
      setModalKind(null)
      await workspace.refresh()
      showNotice(`已创建文件夹“${name}”`)
    } catch (error) {
      showNotice(commandErrorMessage(error, '无法创建文件夹。'))
    } finally {
      setOperationBusy(false)
    }
  }

  const openRename = (file: RemoteFile) => {
    setModalInput(file.name)
    setPendingFiles([file])
    setModalKind('rename')
  }

  const openShare = (file: RemoteFile) => {
    setPendingFiles([file])
    setModalKind('share')
  }

  const renameFile = async () => {
    const file = pendingFiles[0]
    const name = modalInput.trim()
    if (!file || !workspace.activeMountId || !name || name === file.name) return
    setOperationBusy(true)
    try {
      await koofr.renameEntry(workspace.activeMountId, file.path, name)
      setModalKind(null)
      await workspace.refresh()
      showNotice(`已重命名为“${name}”`)
    } catch (error) {
      showNotice(commandErrorMessage(error, '无法重命名这个项目。'))
    } finally {
      setOperationBusy(false)
    }
  }

  const openDelete = (files: RemoteFile[]) => {
    setPendingFiles(files)
    setModalKind('delete')
  }

  const deleteFiles = async () => {
    if (!workspace.activeMountId || pendingFiles.length === 0) return
    setOperationBusy(true)
    try {
      const results = await Promise.allSettled(
        pendingFiles.map((file) => koofr.deleteEntry(workspace.activeMountId, file.path)),
      )
      const deletedCount = results.filter((result) => result.status === 'fulfilled').length
      const failed = results.filter((result) => result.status === 'rejected')
      setModalKind(null)
      if (failed.length === 0) {
        showNotice(`已删除 ${deletedCount} 个项目`)
      } else {
        const prefix = deletedCount > 0 ? `已删除 ${deletedCount} 个项目；` : ''
        showNotice(`${prefix}${commandErrorMessage(failed[0]?.reason, '其余项目删除失败。')}`)
      }
    } finally {
      await workspace.refresh()
      setOperationBusy(false)
    }
  }

  const openMove = (files: RemoteFile[]) => {
    setPendingFiles(files)
    setModalKind('move')
  }

  const executeMove = async (destinationMountId: string, destinationPath: string) => {
    if (!workspace.activeMountId || pendingFiles.length === 0) return
    setOperationBusy(true)
    try {
      const results = await Promise.allSettled(
        pendingFiles.map((file) =>
          koofr.moveEntry(workspace.activeMountId, file.path, destinationMountId, destinationPath),
        ),
      )
      const moved = results.filter((result) => result.status === 'fulfilled').length
      const failed = results.filter((result) => result.status === 'rejected')
      setModalKind(null)
      if (failed.length === 0) {
        showNotice(`已移动 ${moved} 个项目`)
      } else {
        const prefix = moved > 0 ? `已移动 ${moved} 个项目；` : ''
        showNotice(`${prefix}${commandErrorMessage(failed[0]?.reason, '其余项目移动失败。')}`)
      }
    } finally {
      await workspace.refresh()
      setOperationBusy(false)
    }
  }

  const openLocatedFile = (file: LocatedFile) => {
    const segments = file.path.split('/').filter(Boolean)
    const targetPath = isDirectory(file)
      ? file.path
      : segments.length <= 1 ? '/' : `/${segments.slice(0, -1).join('/')}`
    setActiveItem('我的文件')
    void workspace.loadDirectory(file.mountId, targetPath)
  }

  const restoreTrash = async (items: TrashItem[]) => {
    if (items.length === 0) return
    setOperationBusy(true)
    try {
      await koofr.restoreTrash(items)
      await collections.load('回收站', true)
      showNotice(`已提交恢复 ${items.length} 个项目`)
    } catch (error) {
      showNotice(commandErrorMessage(error, '无法恢复所选项目。'))
    } finally {
      setOperationBusy(false)
    }
  }

  const restoreAllTrash = async () => {
    setOperationBusy(true)
    try {
      await koofr.restoreAllTrash()
      await collections.load('回收站', true)
      showNotice('已提交恢复全部项目')
    } catch (error) {
      showNotice(commandErrorMessage(error, '无法恢复回收站项目。'))
    } finally {
      setOperationBusy(false)
    }
  }

  const openEmptyTrash = () => {
    setModalInput('')
    setModalKind('emptyTrash')
  }

  const emptyTrash = async () => {
    if (modalInput !== '永久删除') return
    setOperationBusy(true)
    try {
      await koofr.emptyTrash(modalInput)
      setModalKind(null)
      await collections.load('回收站', true)
      showNotice('回收站已永久清空')
    } catch (error) {
      showNotice(commandErrorMessage(error, '无法清空回收站。'))
    } finally {
      setOperationBusy(false)
    }
  }

  const selectNavigation = (label: string) => {
    setActiveItem(label)
    if (label === '我的文件') {
      if (workspace.activeMountId && workspace.path !== '/') {
        void workspace.loadDirectory(workspace.activeMountId, '/')
      }
      return
    }
  }

  return (
    <div
      className="app-shell"
      data-theme={themeId}
      data-density={settings?.density ?? 'standard'}
      data-reduce-motion={settings?.reduceMotion ? 'true' : 'false'}
    >
      <TitleBar />

      {authState === 'checking' ? (
        <main className="auth-loading" aria-live="polite">
          <span className="auth-spinner" />
          正在检查登录状态…
        </main>
      ) : null}

      {authState === 'signedOut' || authState === 'signingIn' ? (
        <LoginPage
          busy={authState === 'signingIn'}
          error={loginError}
          initialEmail={savedEmail}
          onLogin={login}
          onThemeClick={() => setModalKind('theme')}
        />
      ) : null}

      {authState === 'signedIn' ? (
        <div className="app-body">
          <AppSidebar
            activeItem={activeItem}
            onSelect={selectNavigation}
            onSettingsClick={() => void openSettings()}
            onThemeClick={() => setModalKind('theme')}
            onVaultClick={() => setModalKind('vault')}
            onLogoutClick={logout}
            storageName={activeMount?.name ?? ''}
            storageUsed={activeMount?.spaceUsed ?? null}
            storageTotal={activeMount?.spaceTotal ?? null}
            mounts={workspace.mounts}
          />
          {activeItem === '同步' ? (
            <SyncWorkspace
              mounts={workspace.mounts}
              defaultMountId={workspace.activeMountId}
              defaultRemotePath={workspace.path}
              sync={sync}
            />
          ) : isCollectionView(activeItem) ? (
            <CollectionWorkspace
              view={activeItem}
              files={collections.files}
              trash={collections.trash}
              retentionDays={collections.retentionDays}
              loading={collections.status === 'loading'}
              error={collections.error}
              diagnostic={collections.diagnostic}
              lastSyncedAt={collections.lastSyncedAt}
              busy={operationBusy}
              onRefresh={() => void collections.refresh()}
              onThemeOpen={() => setModalKind('theme')}
              onOpenLocation={openLocatedFile}
              onDownload={(file) => void handleDownload(file, file.mountId)}
              onPreview={(file) => void openFilePreview(
                file,
                file.mountId,
                () => void handleDownload(file, file.mountId),
              )}
              onRestore={(items) => void restoreTrash(items)}
              onRestoreAll={() => void restoreAllTrash()}
              onEmptyTrash={openEmptyTrash}
            />
          ) : (
            <FileWorkspace
              mounts={workspace.mounts}
              activeMountId={workspace.activeMountId}
              path={workspace.path}
              files={workspace.files}
              loading={workspace.status === 'loading'}
              error={workspace.error}
              lastSyncedAt={workspace.lastSyncedAt}
              onMountChange={(mountId) => void workspace.loadDirectory(mountId, '/')}
              onNavigate={(path) => void workspace.loadDirectory(workspace.activeMountId, path)}
              onRefresh={() => void (workspace.activeMountId ? workspace.refresh() : workspace.initialize())}
              onCreateFolder={openCreateFolder}
              onThemeOpen={() => setModalKind('theme')}
              onUpload={(mode) => void handleUpload(mode)}
              onDownload={(file) => void handleDownload(file, workspace.activeMountId)}
              onPreview={(file) => void openFilePreview(
                file,
                workspace.activeMountId,
                () => void handleDownload(file, workspace.activeMountId),
              )}
              onShare={openShare}
              onRename={openRename}
              onMove={openMove}
              onDelete={openDelete}
            />
          )}
          {transferVisible ? (
            <button
              className="transfer-panel-backdrop"
              type="button"
              aria-label="收起传输面板"
              onClick={() => setTransferVisible(false)}
            />
          ) : null}
          <TransferPanel
            visible={transferVisible}
            items={transfers}
            onClose={() => setTransferVisible(false)}
            onCancel={(transferId) => void cancelTransfer(transferId)}
            onPause={(transferId) => void pauseTransfer(transferId)}
            onResume={(transferId) => void resumeTransfer(transferId)}
            onDiscard={(transferId) => void discardResumableTransfer(transferId)}
            onOpenFile={(transferId) => void openDownloadedFile(transferId)}
            onOpenFolder={(transferId) => void openDownloadedFolder(transferId)}
            onClearFinished={() => void clearFinishedTransfers()}
          />
          {!transferVisible ? (
            <button
              className="transfer-reopen"
              type="button"
              aria-label={activeDownloadCount > 0 ? `打开传输界面，${activeDownloadCount} 项正在下载` : '打开传输界面'}
              title="打开传输界面"
              onClick={() => setTransferVisible(true)}
            >
              <ArrowDownToLine size={23} />
              {activeDownloadCount > 0 ? (
                <span>{activeDownloadCount}</span>
              ) : null}
            </button>
          ) : null}
        </div>
      ) : null}

      {notice ? <div className="toast" role="status">{notice}</div> : null}

      {authState === 'signedIn' && modalKind === 'download' && pendingDownload ? (
        <DownloadDestinationDialog
          fileName={pendingDownload.file.name}
          initialDirectory={downloadDirectory}
          initialAskEveryTime={settings?.askDownloadLocation ?? true}
          busy={downloadBusy}
          error={downloadError}
          onBrowse={browseDownloadDirectory}
          onClose={() => {
            setModalKind(null)
            setPendingDownload(null)
            setDownloadError('')
          }}
          onConfirm={(directory, askEveryTime) => void confirmDownload(directory, askEveryTime)}
        />
      ) : null}

      {authState === 'signedIn' && modalKind === 'splitUpload' && pendingSplitUpload ? (
        <SplitUploadDialog
          fileName={pendingSplitUpload.selection.fileName}
          defaultPartMiB={settings?.defaultSplitPartMib ?? 64}
          onClose={() => {
            setModalKind(null)
            setPendingSplitUpload(null)
          }}
          onConfirm={confirmSplitUpload}
        />
      ) : null}

      {authState === 'signedIn' && modalKind === 'createFolder' ? (
        <Modal
          title="新建文件夹"
          actionLabel={operationBusy ? '正在创建…' : '创建文件夹'}
          actionDisabled={operationBusy || !modalInput.trim()}
          onClose={() => setModalKind(null)}
          onAction={() => void createFolder()}
        >
          <label className="modal-field">
            <span>文件夹名称</span>
            <input autoFocus value={modalInput} maxLength={255} onChange={(event) => setModalInput(event.target.value)} />
          </label>
        </Modal>
      ) : null}

      {authState === 'signedIn' && modalKind === 'share' && pendingFiles[0] && workspace.activeMountId ? (
        <CreateShareLinkDialog
          mountId={workspace.activeMountId}
          file={pendingFiles[0]}
          defaultKind={settings?.linkDefaultKind ?? 'download'}
          expireDays={settings?.linkDefaultExpireDays ?? 0}
          requirePassword={settings?.linkDefaultRequirePassword ?? false}
          onClose={() => setModalKind(null)}
        />
      ) : null}

      {authState === 'signedIn' && modalKind === 'rename' ? (
        <Modal
          title="重命名"
          actionLabel={operationBusy ? '正在保存…' : '保存名称'}
          actionDisabled={operationBusy || !modalInput.trim() || modalInput.trim() === pendingFiles[0]?.name}
          onClose={() => setModalKind(null)}
          onAction={() => void renameFile()}
        >
          <label className="modal-field">
            <span>新名称</span>
            <input autoFocus value={modalInput} maxLength={255} onChange={(event) => setModalInput(event.target.value)} />
          </label>
        </Modal>
      ) : null}

      {authState === 'signedIn' && modalKind === 'delete' ? (
        <Modal
          title="确认删除"
          actionLabel={operationBusy ? '正在删除…' : `删除 ${pendingFiles.length} 个项目`}
          actionDisabled={operationBusy}
          onClose={() => setModalKind(null)}
          onAction={() => void deleteFiles()}
        >
          <p>删除后，所选文件和文件夹将从当前 Koofr 存储位置移除。此操作需要明确确认。</p>
        </Modal>
      ) : null}

      {authState === 'signedIn' && modalKind === 'move' && pendingFiles.length > 0 && workspace.activeMountId ? (
        <MoveDestinationDialog
          files={pendingFiles}
          mountId={workspace.activeMountId}
          mounts={workspace.mounts}
          busy={operationBusy}
          onClose={() => setModalKind(null)}
          onConfirm={(mntId, path) => void executeMove(mntId, path)}
        />
      ) : null}

      {authState === 'signedIn' && modalKind === 'emptyTrash' ? (
        <Modal
          title="永久清空回收站"
          actionLabel={operationBusy ? '正在永久删除…' : '永久清空'}
          actionDisabled={operationBusy || modalInput !== '永久删除'}
          onClose={() => setModalKind(null)}
          onAction={() => void emptyTrash()}
        >
          <p>回收站中的所有项目将被永久删除，且无法恢复。请输入“永久删除”以确认。</p>
          <label className="modal-field modal-field--spaced">
            <span>确认文字</span>
            <input autoFocus value={modalInput} onChange={(event) => setModalInput(event.target.value)} />
          </label>
        </Modal>
      ) : null}

      {authState === 'signedIn' && modalKind === 'vault' ? (
        <VaultDialog
          mountId={workspace.activeMountId}
          parentPath={workspace.path}
          onClose={() => setModalKind(null)}
          onNotice={showNotice}
          onTransferStarted={startVaultTransfer}
          onPreview={(repoId, entry, onDownload) => {
            void openVaultFilePreview(repoId, entry, onDownload)
          }}
        />
      ) : null}

      {authState === 'signedIn' && modalKind === 'floatingPrompt' ? (
        <Modal
          title="显示传输悬浮窗？"
          actionLabel={settingsBusy ? '正在开启…' : '开启悬浮窗'}
          actionDisabled={settingsBusy}
          secondaryActionLabel="暂不开启"
          secondaryActionDisabled={settingsBusy}
          onClose={() => {
            if (!settingsBusy) void respondToFloatingPrompt(false)
          }}
          onSecondaryAction={() => void respondToFloatingPrompt(false)}
          onAction={() => void respondToFloatingPrompt(true)}
        >
          <div className="floating-window-prompt">
            <p>
              悬浮窗会置顶显示上传、下载和暂停状态，只同步任务数量与汇总百分比，不包含文件名或路径。
            </p>
            <p>可拖动到合适位置，单击即可回到传输面板。</p>
          </div>
        </Modal>
      ) : null}

      {authState === 'signedIn' && modalKind === 'settings' ? (
        <Modal title="设置" actionLabel="完成" extraWide onClose={() => setModalKind(null)}>
          <SettingsPanel
            settings={settings}
            loading={settingsLoading}
            busy={settingsBusy}
            error={settingsError}
            downloadError={downloadSettingsError}
            onCacheModeChange={(cacheMode) => {
              if (settings) void updateCacheSettings(
                cacheMode,
                settings.cacheTtlMinutes,
              )
            }}
            onCacheTtlChange={(cacheTtlMinutes) => {
              if (settings) void updateCacheSettings(
                settings.cacheMode,
                cacheTtlMinutes,
              )
            }}
            onLoggingSettingsChange={(next) => void updateLoggingSettings(
              next.logLevel,
              next.logRetentionDays,
              next.logMaxFileSizeMb,
            )}
            onTransferSettingsChange={(next) => void updateTransferSettings(
              next.autoRetryNetworkErrors,
              next.networkRetryLimit,
              next.networkRetryIntervalSeconds,
            )}
            onDownloadSettingsChange={(directory, askDownloadLocation) => {
              void updateDownloadSettings(directory, askDownloadLocation)
            }}
            onBrowseDownloadDirectory={browseSettingsDownloadDirectory}
            onBrowseWorkDirectory={browseWorkDirectory}
            onWorkDirectoryChange={(directory, moveExisting) => {
              void updateWorkDirectory(directory, moveExisting)
            }}
            onClearCache={() => void clearMetadataCache()}
            onClearLogs={() => void clearLogs()}
            onForgetLogin={() => void forgetSavedLogin()}
            themeId={themeId}
            onThemeChange={selectTheme}
            onAppearanceChange={updateAppearance}
            onGeneralChange={updateGeneral}
            onTransferAdvancedChange={updateAdvancedTransfer}
            onDownloadBehaviorChange={updateDownloadBehavior}
            onBandwidthChange={updateBandwidth}
            onNotificationChange={updateNotifications}
            onLinkDefaultsChange={updateLinkDefaults}
            onSecurityChange={updateSecurity}
            onHistoryPolicyChange={updateHistoryPolicy}
            onClearDownloadHistory={() => void clearFinishedTransfers()}
            versionInfo={versionInfo}
            onOpenExternalPage={(page) => {
              void koofr.openExternalPage(page).catch((error) => {
                setSettingsError(commandErrorMessage(error, '无法打开外部页面。'))
              })
            }}
          />
        </Modal>
      ) : null}

      {modalKind === 'theme' ? (
        <Modal title="外观主题" actionLabel="完成" onClose={() => setModalKind(null)}>
          <ThemePicker value={themeId} onChange={selectTheme} />
        </Modal>
      ) : null}

      {filePreview ? (
        <FilePreviewModal
          previewInfo={filePreview.previewInfo}
          fileName={filePreview.fileName}
          onClose={closeFilePreview}
          onDownload={downloadPreviewedFile}
        />
      ) : null}
    </div>
  )
}

export default App
