import { useCallback, useEffect, useState } from 'react'
import { beginDownload } from '../features/transfers/beginDownload'
import {
  shouldPromptForFloatingWindow,
  summarizeFloatingTransfers,
} from '../features/transfers/floatingTransfer'
import {
  mergeTransferProgress,
} from '../features/transfers/transferProgress'
import {
  commandErrorMessage,
  isCommandErrorCode,
  isTauriRuntime,
  koofr,
  showTransferNotification,
} from '../services/koofr'
import type { ThemeId } from '../theme'
import type {
  AppSettings,
  DownloadHistoryItem,
  LocalFileSelection,
  RemoteFile,
  ResumableTransfer,
  TransferProgress,
  TransferResult,
  SyncPairSummary,
} from '../types/backend'
import type { SplitUploadSettings, TransferItem } from '../types/files'
import type { VaultTransferSpec } from '../types/vault'

export function useTransferManager(
  authState: string,
  settings: AppSettings | null,
  themeId: ThemeId,
  workspaceRefresh: () => Promise<void>,
  showNotice: (message: string) => void,
  openDownloadDestination: (
    file: RemoteFile,
    mountId: string,
    fallbackDirectory: string,
    errorMessage?: string,
  ) => void,
  syncControl?: {
    readonly pairs: readonly SyncPairSummary[]
    readonly completionVersion?: number
    readonly setPairPaused: (pairId: string, paused: boolean) => Promise<void>
  },
) {
  const [transfers, setTransfers] = useState<TransferItem[]>([])
  const [transferVisible, setTransferVisible] = useState(false)
  const [floatingCompletionVersion, setFloatingCompletionVersion] = useState(0)
  const [downloadBusy, setDownloadBusy] = useState(false)
  const [downloadError, setDownloadError] = useState('')
  const [downloadDirectory, setDownloadDirectory] = useState('')

  const activeTransferCount = transfers.filter(
    (item) =>
      item.state === 'queued' ||
      item.state === 'running' ||
      item.state === 'retrying',
  ).length

  // Taskbar badge update
  useEffect(() => {
    if (!isTauriRuntime()) return
    void koofr
      .setTaskbarBadge(activeTransferCount, settings?.trayBadge ?? false)
      .catch(() => {})
  }, [activeTransferCount, settings?.trayBadge])

  // Transfer progress event listener
  useEffect(() => {
    if (authState !== 'signedIn') return
    let disposed = false
    let unlisten: (() => void) | undefined

    void koofr
      .onTransferProgress((progress: TransferProgress) => {
        if (progress.state === 'completed') {
          setFloatingCompletionVersion((current) => current + 1)
        }
        setTransfers((current) => {
          const previous = current.find((item) => item.id === progress.transferId)
          const merged = mergeTransferProgress(current, progress)
          if (
            settings?.notifyOnComplete &&
            previous &&
            previous.state !== 'completed' &&
            progress.state === 'completed'
          ) {
            showTransferNotification(
              previous.name,
              settings?.notifySound ?? false,
            )
          }
          if (
            settings?.openFolderAfterDownload &&
            previous?.direction === 'download' &&
            previous.state !== 'completed' &&
            progress.state === 'completed'
          ) {
            void koofr
              .openDownloadedFolder(progress.transferId)
              .catch((error) => {
                showNotice(
                  commandErrorMessage(
                    error,
                    '下载已完成，但无法打开所在文件夹。',
                  ),
                )
              })
          }
          return merged
        })
      })
      .then((stopListening) => {
        if (disposed) stopListening()
        else unlisten = stopListening
      })

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [
    authState,
    settings?.notifyOnComplete,
    settings?.notifySound,
    settings?.openFolderAfterDownload,
    showNotice,
  ])

  // Publish floating transfer snapshot
  useEffect(() => {
    if (!isTauriRuntime()) return
    const summary = summarizeFloatingTransfers(transfers, syncControl?.pairs ?? [])
    void koofr
      .publishFloatingTransferSnapshot({
        ...summary,
        completionVersion: floatingCompletionVersion + (syncControl?.completionVersion ?? 0),
        language: settings?.language ?? 'system',
        reduceMotion: settings?.reduceMotion ?? false,
        themeId,
        compactEnabled: settings?.floatingWindowCompactEnabled ?? true,
        clickToggleEnabled: settings?.floatingWindowClickToggleEnabled ?? true,
        doubleClickOpenEnabled:
          settings?.floatingWindowDoubleClickOpenEnabled ?? true,
        resumablePausedCount: transfers.filter(
          (item) => item.state === 'paused' && item.recoveryKind !== null,
        ).length,
      })
      .catch(() => {})
  }, [
    floatingCompletionVersion,
    syncControl?.completionVersion,
    settings?.language,
    settings?.reduceMotion,
    settings?.floatingWindowCompactEnabled,
    settings?.floatingWindowClickToggleEnabled,
    settings?.floatingWindowDoubleClickOpenEnabled,
    themeId,
    transfers,
    syncControl?.pairs,
  ])

  // Restore history and checkpoints on sign in
  useEffect(() => {
    let active = true
    if (authState !== 'signedIn') {
      setTransfers([])
      return () => {
        active = false
      }
    }

    void Promise.all([
      koofr.listDownloadHistory(),
      koofr.listResumableTransfers().catch(() => []),
    ])
      .then(([history, checkpoints]: [DownloadHistoryItem[], ResumableTransfer[]]) => {
        if (!active) return
        setTransfers((current) => {
          const cachedById = new Map(current.map((item) => [item.id, item]))
          const cachedCheckpoints = new Map(
            checkpoints.map((item) => [item.transferId, item]),
          )
          const restored: TransferItem[] = history
            .filter((item) => !cachedById.has(item.transferId))
            .map((item) => ({
              id: item.transferId,
              name: item.name,
              direction: 'download' as const,
              state: item.state,
              bytesTransferred: item.bytesTransferred,
              totalBytes: item.totalBytes,
              localKind: cachedById.get(item.transferId)?.localKind ?? 'file',
              recoveryKind:
                item.recoveryKind ??
                cachedCheckpoints.get(item.transferId)?.recoveryKind ??
                null,
              remotePath: cachedById.get(item.transferId)?.remotePath ?? null,
              localPath: cachedById.get(item.transferId)?.localPath ?? null,
              startedAt:
                cachedById.get(item.transferId)?.startedAt ?? item.startedAt,
              finishedAt:
                cachedById.get(item.transferId)?.finishedAt ?? item.finishedAt,
              speedSamples:
                cachedById.get(item.transferId)?.speedSamples ??
                item.speedSamples,
              splitProgress:
                cachedCheckpoints.get(item.transferId)?.splitProgress ??
                undefined,
              integrityStatus: item.integrityStatus,
            }))
          const resumable: TransferItem[] = checkpoints.map((item) => ({
            id: item.transferId,
            name: item.name,
            direction: item.direction,
            state: 'paused' as const,
            bytesTransferred: item.bytesTransferred,
            totalBytes: item.totalBytes,
            localKind: cachedById.get(item.transferId)?.localKind ?? 'file',
            recoveryKind: item.recoveryKind,
            remotePath: cachedById.get(item.transferId)?.remotePath ?? null,
            localPath: cachedById.get(item.transferId)?.localPath ?? null,
            startedAt: cachedById.get(item.transferId)?.startedAt ?? null,
            finishedAt: cachedById.get(item.transferId)?.finishedAt ?? null,
            speedSamples: cachedById.get(item.transferId)?.speedSamples ?? [],
            splitProgress: item.splitProgress,
          }))
          const resumableIds = new Set(resumable.map((item) => item.id))
          return [
            ...resumable,
            ...restored.filter((item) => !resumableIds.has(item.id)),
            ...current,
          ].sort(
            (left, right) => (right.startedAt ?? 0) - (left.startedAt ?? 0),
          )
        })
      })
      .catch((error) => {
        if (active)
          showNotice(commandErrorMessage(error, '无法读取中断的传输任务。'))
      })

    return () => {
      active = false
    }
  }, [authState, showNotice])

  const startDownload = useCallback(
    async (file: RemoteFile, mountId: string, targetDirectory: string) => {
      const handle = await beginDownload(file, mountId, targetDirectory)
      const startedAt = Date.now()

      setTransfers((current) => [
        {
          id: handle.transferId,
          name: file.name,
          direction: 'download' as const,
          state: 'running' as const,
          bytesTransferred: 0,
          totalBytes: file.size ?? null,
          localKind: file.entryType === 'dir' ? 'folder' : 'file',
          recoveryKind: file.entryType === 'dir' ? null : 'byte_resume',
          remotePath: file.path,
          localPath: targetDirectory,
          startedAt,
          finishedAt: null,
          speedSamples: [{ recordedAt: startedAt, bytesTransferred: 0 }],
        },
        ...current,
      ])
      setTransferVisible(true)

      try {
        const result = await handle.result
        setTransfers((current) =>
          current.map((item) =>
            item.id === handle.transferId
              ? {
                  ...item,
                  state: 'completed',
                  bytesTransferred: result.bytesTransferred,
                  integrityStatus: result.integrityStatus,
                  totalBytes: item.totalBytes ?? result.bytesTransferred,
                  finishedAt: Date.now(),
                }
              : item,
          ),
        )
      } catch (error) {
        setTransfers((current) =>
          current.map((item) =>
            item.id === handle.transferId
              ? {
                  ...item,
                  state: isCommandErrorCode(error, 'transfer_paused')
                    ? 'paused'
                    : isCommandErrorCode(error, 'cancelled')
                      ? 'cancelled'
                      : 'failed',
                  finishedAt: null,
                }
              : item,
          ),
        )
        showNotice(commandErrorMessage(error, '下载失败，请稍后重试。'))
      }
    },
    [showNotice],
  )

  const handleDownload = useCallback(
    async (file: RemoteFile, mountId: string) => {
      try {
        const downloadSettings = settings
          ? {
              downloadDirectory: settings.downloadDirectory,
              askDownloadLocation: settings.askDownloadLocation,
            }
          : await koofr.getSettings()

        if (
          shouldPromptForFloatingWindow(
            authState === 'signedIn',
            settings !== null,
            settings?.floatingWindowPrompted ?? false,
            false,
          )
        ) {
          void koofr
            .updateNotificationSettings(
              settings?.notifyOnComplete ?? true,
              settings?.notifySound ?? true,
              settings?.trayBadge ?? true,
              settings?.floatingWindowEnabled ?? false,
              true,
              settings?.floatingWindowCompactEnabled ?? true,
              settings?.floatingWindowClickToggleEnabled ?? true,
              settings?.floatingWindowDoubleClickOpenEnabled ?? true,
            )
            .catch(() => {})
        }

        if (downloadSettings.askDownloadLocation) {
          openDownloadDestination(
            file,
            mountId,
            downloadSettings.downloadDirectory,
          )
          return
        }
        await startDownload(file, mountId, downloadSettings.downloadDirectory)
      } catch (error) {
        openDownloadDestination(
          file,
          mountId,
          '',
          commandErrorMessage(error, '默认下载位置不可用，请选择其他文件夹。'),
        )
      }
    },
    [authState, openDownloadDestination, settings, startDownload],
  )

  const browseDownloadDirectory = useCallback(async () => {
    setDownloadError('')
    try {
      return await koofr.selectDownloadDirectory()
    } catch (error) {
      setDownloadError(
        commandErrorMessage(error, '无法打开下载文件夹选择器。'),
      )
      return null
    }
  }, [])

  const cancelTransfer = useCallback(
    async (transferId: string) => {
      try {
        await koofr.cancelTransfer(transferId)
      } catch (error) {
        showNotice(commandErrorMessage(error, '无法取消这个传输任务。'))
      }
    },
    [showNotice],
  )

  const pauseTransfer = useCallback(
    async (transferId: string) => {
      try {
        await koofr.pauseTransfer(transferId)
      } catch (error) {
        showNotice(commandErrorMessage(error, '无法暂停这个传输任务。'))
      }
    },
    [showNotice],
  )

  const resumeTransfer = useCallback(
    async (transferId: string, revealPanel = true) => {
      const resumedAt = Date.now()
      setTransfers((current) =>
        current.map((item) =>
          item.id === transferId
            ? {
                ...item,
                state: 'running',
                speedSamples:
                  item.speedSamples.length > 0
                    ? item.speedSamples
                    : [{ recordedAt: resumedAt, bytesTransferred: item.bytesTransferred }],
              }
            : item,
        ),
      )
      if (revealPanel) setTransferVisible(true)
      try {
        const result = await koofr.resumeTransfer(transferId)
        setTransfers((current) =>
          current.map((item) =>
            item.id === transferId
              ? {
                  ...item,
                  state: 'completed',
                  bytesTransferred: result.bytesTransferred,
                  integrityStatus: result.integrityStatus,
                  totalBytes: item.totalBytes ?? result.bytesTransferred,
                  finishedAt: Date.now(),
                }
              : item,
          ),
        )
        await workspaceRefresh()
      } catch (error) {
        setTransfers((current) =>
          current.map((item) =>
            item.id === transferId
              ? {
                  ...item,
                  state: isCommandErrorCode(error, 'transfer_paused')
                    ? 'paused'
                    : 'failed',
                  finishedAt: null,
                }
              : item,
          ),
        )
        showNotice(commandErrorMessage(error, '无法继续这个传输任务。'))
      }
    },
    [showNotice, workspaceRefresh],
  )

  const toggleFloatingTransfers = useCallback(() => {
    const active = transfers.filter(
      (item) =>
        item.state === 'queued' ||
        item.state === 'running' ||
        item.state === 'retrying',
    )
    const activeSync = (syncControl?.pairs ?? []).filter((pair) =>
      !pair.paused && (pair.status === 'initializing' || pair.status === 'scanning' || pair.status === 'syncing'),
    )
    if (active.length > 0 || activeSync.length > 0) {
      void Promise.allSettled([
        ...active.map((item) => koofr.pauseTransfer(item.id)),
        ...(syncControl ? activeSync.map((pair) => syncControl.setPairPaused(pair.id, true)) : []),
      ]).then((results) => {
        if (results.some((result) => result.status === 'rejected')) {
          setTransferVisible(true)
          showNotice('部分传输无法暂停，请在传输面板中查看。')
        }
      })
      return
    }

    const paused = transfers.filter((item) => item.state === 'paused')
    const resumable = paused.filter((item) => item.recoveryKind !== null)
    const pausedSync = syncControl?.pairs.filter((pair) => pair.paused || pair.status === 'paused') ?? []
    if (resumable.length === 0 && pausedSync.length === 0) {
      if (paused.length > 0) {
        setTransferVisible(true)
        showNotice('这些暂停任务需要在传输面板中处理。')
      }
      return
    }
    for (const item of resumable) void resumeTransfer(item.id, false)
    if (syncControl) {
      for (const pair of pausedSync) {
        void syncControl.setPairPaused(pair.id, false).catch(() => showNotice('部分同步组无法继续，请在同步页面中查看。'))
      }
    }
    if (resumable.length !== paused.length || pausedSync.length > 0) {
      setTransferVisible(true)
      showNotice('已继续可恢复任务；其余任务需要在传输面板中处理。')
    }
  }, [resumeTransfer, showNotice, syncControl, transfers])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let disposed = false
    let unlisten: (() => void) | undefined
    void koofr
      .onFloatingTransferAction((action) => {
        if (!disposed && action === 'toggle') toggleFloatingTransfers()
      })
      .then((stopListening) => {
        if (disposed) stopListening()
        else unlisten = stopListening
      })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [toggleFloatingTransfers])

  const discardResumableTransfer = useCallback(
    async (transferId: string) => {
      try {
        await koofr.discardResumableTransfer(transferId)
        setTransfers((current) =>
          current.filter((item) => item.id !== transferId),
        )
      } catch (error) {
        showNotice(commandErrorMessage(error, '无法放弃这个中断任务。'))
      }
    },
    [showNotice],
  )

  const clearFinishedTransfers = useCallback(async () => {
    try {
      await koofr.clearFinishedDownloadHistory()
      setTransfers((current) =>
        current.filter(
          (item) =>
            item.state === 'queued' ||
            item.state === 'running' ||
            item.state === 'retrying' ||
            item.state === 'paused' ||
            (item.state === 'failed' && item.recoveryKind !== null),
        ),
      )
    } catch (error) {
      showNotice(commandErrorMessage(error, '无法清除下载历史。'))
    }
  }, [showNotice])

  const openDownloadedFile = useCallback(
    async (transferId: string) => {
      try {
        await koofr.openDownloadedFile(transferId)
      } catch (error) {
        showNotice(commandErrorMessage(error, '无法打开下载的文件。'))
      }
    },
    [showNotice],
  )

  const openDownloadedFolder = useCallback(
    async (transferId: string) => {
      try {
        await koofr.openDownloadedFolder(transferId)
      } catch (error) {
        showNotice(commandErrorMessage(error, '无法打开下载所在的文件夹。'))
      }
    },
    [showNotice],
  )

  const startVaultTransfer = useCallback(
    (transfer: VaultTransferSpec) => {
      const startedAt = Date.now()
      setTransfers((current) => [
        {
          id: transfer.transferId,
          name: transfer.name,
          direction: transfer.direction,
          state: 'running',
          bytesTransferred: 0,
          totalBytes: transfer.totalBytes,
          localKind: 'file',
          recoveryKind: transfer.recoveryKind,
          remotePath: null,
          localPath: transfer.localPath,
          startedAt,
          finishedAt: null,
          speedSamples: [{ recordedAt: startedAt, bytesTransferred: 0 }],
        },
        ...current,
      ])
      setTransferVisible(true)

      void transfer.result
        .then((result) => {
          setTransfers((current) =>
            current.map((item) =>
              item.id === transfer.transferId
                ? {
                    ...item,
                    state: 'completed',
                    bytesTransferred: result.bytesTransferred,
                    integrityStatus: result.integrityStatus,
                    totalBytes: item.totalBytes ?? result.bytesTransferred,
                    finishedAt: Date.now(),
                  }
                : item,
            ),
          )
        })
        .catch((error) => {
          setTransfers((current) =>
            current.map((item) =>
              item.id === transfer.transferId
                ? {
                    ...item,
                    state: isCommandErrorCode(error, 'transfer_paused')
                      ? 'paused'
                      : isCommandErrorCode(error, 'cancelled')
                        ? 'cancelled'
                        : 'failed',
                    finishedAt: null,
                  }
                : item,
            ),
          )
          showNotice(
            commandErrorMessage(
              error,
              transfer.direction === 'upload'
                ? '加密上传失败，请稍后重试。'
                : '加密下载失败，请稍后重试。',
            ),
          )
        })
    },
    [showNotice],
  )

  const startSelectedUpload = useCallback(
    async (
      selection: LocalFileSelection,
      uploadMountId: string,
      uploadPath: string,
      splitSettings: SplitUploadSettings | null,
    ) => {
      const transfer = splitSettings
        ? koofr.uploadSplitFile(
            uploadMountId,
            uploadPath,
            selection.grantId,
            splitSettings,
          )
        : koofr.uploadFile(uploadMountId, uploadPath, selection.grantId)
      const startedAt = Date.now()
      setTransfers((current) => [
        {
          id: transfer.transferId,
          name: selection.fileName,
          direction: 'upload',
          state: 'running',
          bytesTransferred: 0,
          totalBytes: null,
          localKind: 'file',
          recoveryKind: splitSettings ? 'chunk_resume' : 'restart',
          remotePath: null,
          localPath: null,
          startedAt,
          finishedAt: null,
          speedSamples: [{ recordedAt: startedAt, bytesTransferred: 0 }],
          splitProgress:
            splitSettings && selection.fileSize
              ? {
                  completedParts: 0,
                  currentPart: 1,
                  totalParts: Math.ceil(
                    selection.fileSize / splitSettings.partBytes,
                  ),
                  phase: 'parts',
                }
              : undefined,
        },
        ...current,
      ])
      setTransferVisible(true)

      let result: TransferResult
      try {
        result = await transfer.result
      } catch (error) {
        setTransfers((current) =>
          current.map((item) =>
            item.id === transfer.transferId
              ? {
                  ...item,
                  state: isCommandErrorCode(error, 'transfer_paused')
                    ? 'paused'
                    : isCommandErrorCode(error, 'cancelled')
                      ? 'cancelled'
                      : 'failed',
                }
              : item,
          ),
        )
        showNotice(commandErrorMessage(error, '上传失败，请稍后重试。'))
        return
      }
      setTransfers((current) =>
        current.map((item) =>
          item.id === transfer.transferId
            ? {
                ...item,
                state: 'completed',
                bytesTransferred: result.bytesTransferred,
                integrityStatus: result.integrityStatus,
                totalBytes: item.totalBytes ?? result.bytesTransferred,
                finishedAt: Date.now(),
              }
            : item,
        ),
      )
      await workspaceRefresh()
    },
    [showNotice, workspaceRefresh],
  )

  return {
    transfers,
    setTransfers,
    transferVisible,
    setTransferVisible,
    floatingCompletionVersion,
    setFloatingCompletionVersion,
    downloadBusy,
    setDownloadBusy,
    downloadError,
    setDownloadError,
    downloadDirectory,
    setDownloadDirectory,
    startDownload,
    handleDownload,
    browseDownloadDirectory,
    cancelTransfer,
    pauseTransfer,
    resumeTransfer,
    toggleFloatingTransfers,
    discardResumableTransfer,
    clearFinishedTransfers,
    openDownloadedFile,
    openDownloadedFolder,
    startVaultTransfer,
    startSelectedUpload,
  }
}
