import { invoke } from '@tauri-apps/api/core'
import { emitTo, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification'
import type {
  AppSettings,
  AppVersionInfo,
  BandwidthMode,
  CacheMode,
  DownloadConflictStrategy,
  InterfaceLanguage,
  LinkDefaultKind,
  LogLevel,
  UiDensity,
  KoofrMount,
  KoofrSession,
  LocatedFile,
  LocalFileSelection,
  LoginBootstrap,
  RemoteFile,
  ResumableTransfer,
  TransferProgress,
  TransferResult,
  TrashItem,
  TrashList,
  CommandError,
  DownloadHistoryItem,
  VaultDirectory,
  VaultSummary,
  FilePreviewInfo,
  PreviewTextResult,
  SyncIssue,
  SyncPairSummary,
  SyncTaskSummary,
  SyncStateEvent,
  NativeSyncCapability,
  SyncRootKind,
} from '../types/backend'
import type { SplitUploadSettings } from '../types/files'
import type { FloatingTransferSnapshot } from '../features/transfers/floatingTransfer'

const TRANSFER_EVENT = 'koofr://transfer-progress'
const FLOATING_TRANSFER_EVENT = 'koofr://floating-transfer-state'
const FLOATING_WINDOW_DISABLED_EVENT = 'koofr://floating-window-disabled'
const FLOATING_TRANSFER_ACTION_EVENT = 'koofr://floating-transfer-action'
const SYNC_STATE_EVENT = 'koofr://sync-state'

export type FloatingTransferAction = 'toggle'

export function isTauriRuntime() {
  return '__TAURI_INTERNALS__' in window
}

export async function ensureNotificationPermission() {
  if (await isPermissionGranted()) return true
  return (await requestPermission()) === 'granted'
}

export function showTransferNotification(displayName: string, sound: boolean) {
  const english = document.documentElement.lang === 'en-US'
  sendNotification({
    title: english ? 'Koofr transfer complete' : 'Koofr 传输完成',
    body: displayName || (english ? 'The file transfer is complete.' : '文件传输已完成'),
    sound: sound ? 'Default' : undefined,
  })
}

export function commandErrorMessage(error: unknown, fallback: string) {
  if (
    typeof error === 'object'
    && error !== null
    && typeof (error as Partial<CommandError>).code === 'string'
    && typeof (error as Partial<CommandError>).message === 'string'
  ) {
    const commandError = error as CommandError
    if (document.documentElement.lang === 'en-US') {
      return englishErrorMessages[commandError.code] ?? fallback
    }
    return commandError.message
  }
  return fallback
}

const englishErrorMessages: Partial<Record<CommandError['code'], string>> = {
  authentication_failed: 'The email or app password is incorrect.',
  not_authenticated: 'Connect a Koofr account first.',
  account_identity_unavailable: 'The current Koofr account could not be verified.',
  invalid_input: 'The request contains an invalid path, name, or identifier.',
  conflict: 'The target already exists or the operation is already active.',
  not_found: 'The requested file, folder, or mount was not found.',
  forbidden: 'This account is not allowed to perform the operation.',
  cancelled: 'The transfer was cancelled.',
  transfer_paused: 'The transfer is paused and can be resumed from Transfers.',
  incomplete_transfer: 'The transfer ended before all bytes were received.',
  integrity_error: 'Integrity verification failed. The file was not published.',
  remote_error: 'Koofr could not complete the operation.',
  network_error: 'Koofr could not be reached. Check the network and try again.',
  local_io_error: 'The selected local file could not be read or written.',
  invalid_response: 'Koofr returned data the app could not understand.',
  initialization_error: 'The app backend could not be initialized.',
  dialog_error: 'The native file picker could not be opened.',
  local_open_error: 'The downloaded item could not be opened.',
  local_data_error: 'Local application data could not be read or saved.',
  credential_store_error: 'Windows Credential Manager could not be accessed.',
  vault_locked: 'The Vault is locked.',
  vault_invalid_key: 'The Safe Key is incorrect.',
  vault_crypto_error: 'The Vault data failed cryptographic validation.',
  vault_prompt_error: 'The native Safe Key prompt could not be opened.',
}

export function commandErrorDiagnostic(error: unknown) {
  if (
    typeof error === 'object'
    && error !== null
    && typeof (error as Partial<CommandError>).diagnostic === 'string'
  ) {
    return (error as CommandError).diagnostic ?? ''
  }
  return ''
}

export function isCommandErrorCode(error: unknown, code: CommandError['code']) {
  return typeof error === 'object'
    && error !== null
    && (error as Partial<CommandError>).code === code
}

export const koofr = {
  connect(email: string, appPassword: string, rememberPassword: boolean) {
    return invoke<KoofrSession>('connect_koofr', { email, appPassword, rememberPassword })
  },

  restoreSavedLogin() {
    return invoke<LoginBootstrap>('restore_saved_login')
  },

  disconnect() {
    return invoke<void>('disconnect_koofr')
  },

  session() {
    return invoke<KoofrSession>('koofr_session')
  },

  getSettings() {
    return invoke<AppSettings>('get_settings')
  },

  updateSettings(cacheMode: CacheMode, cacheTtlMinutes: number) {
    return invoke<AppSettings>('update_settings', {
      cacheMode,
      cacheTtlMinutes,
    })
  },

  updateLoggingSettings(
    logLevel: LogLevel,
    logRetentionDays: number,
    logMaxFileSizeMb: number,
  ) {
    return invoke<AppSettings>('update_logging_settings', {
      logLevel,
      logRetentionDays,
      logMaxFileSizeMb,
    })
  },

  updateTransferSettings(
    autoRetryNetworkErrors: boolean,
    networkRetryLimit: number | null,
    networkRetryIntervalSeconds: number,
  ) {
    return invoke<AppSettings>('update_transfer_settings', {
      autoRetryNetworkErrors,
      networkRetryLimit,
      networkRetryIntervalSeconds,
    })
  },

  updateDownloadSettings(downloadDirectory: string, askDownloadLocation: boolean) {
    return invoke<AppSettings>('update_download_settings', {
      downloadDirectory,
      askDownloadLocation,
    })
  },

  updateWorkDirectory(workDirectory: string, moveExisting: boolean) {
    return invoke<AppSettings>('update_work_directory', {
      workDirectory,
      moveExisting,
    })
  },

  clearMetadataCache() {
    return invoke<AppSettings>('clear_metadata_cache')
  },

  clearLogs() {
    return invoke<AppSettings>('clear_logs')
  },

  forgetSavedLogin() {
    return invoke<AppSettings>('forget_saved_login')
  },

  updateAppearanceSettings(
    language: InterfaceLanguage,
    reduceMotion: boolean,
    density: UiDensity,
  ) {
    return invoke<AppSettings>('update_appearance_settings', {
      language,
      reduceMotion,
      density,
    })
  },

  updateGeneralSettings(
    launchAtLogin: boolean,
    closeToTray: boolean,
    minimizeToTray: boolean,
    confirmQuitWithTransfers: boolean,
  ) {
    return invoke<AppSettings>('update_general_settings', {
      launchAtLogin,
      closeToTray,
      minimizeToTray,
      confirmQuitWithTransfers,
    })
  },

  updateTransferAdvancedSettings(
    uploadConcurrency: number,
    downloadConcurrency: number,
    defaultSplitPartMib: number,
    autoSplitThresholdMib: number,
  ) {
    return invoke<AppSettings>('update_transfer_advanced_settings', {
      uploadConcurrency,
      downloadConcurrency,
      defaultSplitPartMib,
      autoSplitThresholdMib,
    })
  },

  updateDownloadBehaviorSettings(
    conflictStrategy: DownloadConflictStrategy,
    openFolderAfterDownload: boolean,
    verifyHashAfterDownload: boolean,
    keepFailedPartDays: number,
  ) {
    return invoke<AppSettings>('update_download_behavior_settings', {
      conflictStrategy,
      openFolderAfterDownload,
      verifyHashAfterDownload,
      keepFailedPartDays,
    })
  },

  updateBandwidthSettings(globalKbps: number, perTaskKbps: number, mode: BandwidthMode) {
    return invoke<AppSettings>('update_bandwidth_settings', {
      globalKbps,
      perTaskKbps,
      mode,
    })
  },

  updateNotificationSettings(
    notifyOnComplete: boolean,
    notifySound: boolean,
    trayBadge: boolean,
    floatingWindowEnabled: boolean,
    floatingWindowPrompted: boolean,
    floatingWindowCompactEnabled: boolean,
    floatingWindowClickToggleEnabled: boolean,
    floatingWindowDoubleClickOpenEnabled: boolean,
  ) {
    return invoke<AppSettings>('update_notification_settings', {
      notifyOnComplete,
      notifySound,
      trayBadge,
      floatingWindowEnabled,
      floatingWindowPrompted,
      floatingWindowCompactEnabled,
      floatingWindowClickToggleEnabled,
      floatingWindowDoubleClickOpenEnabled,
    })
  },

  updateLinkDefaults(
    kind: LinkDefaultKind,
    expireDays: number,
    requirePassword: boolean,
  ) {
    return invoke<AppSettings>('update_link_defaults', {
      kind,
      expireDays,
      requirePassword,
    })
  },

  updateSecuritySettings(idleAutoLogoutMinutes: number) {
    return invoke<AppSettings>('update_security_settings', {
      idleAutoLogoutMinutes,
    })
  },

  updateHistoryPolicy(keepDays: number, keepCount: number) {
    return invoke<AppSettings>('update_history_policy', {
      keepDays,
      keepCount,
    })
  },

  appVersionInfo() {
    return invoke<AppVersionInfo>('app_version_info')
  },

  openExternalPage(page: 'homepage' | 'releases' | 'issues') {
    return invoke<void>('open_external_page', { page })
  },

  setTaskbarBadge(count: number, enabled: boolean) {
    return invoke<void>('set_taskbar_badge', { count, enabled })
  },

  listMounts(refresh = false) {
    return invoke<KoofrMount[]>('list_mounts', { refresh })
  },

  listFiles(mountId: string, path = '/', refresh = false) {
    return invoke<RemoteFile[]>('list_files', { mountId, path, refresh })
  },

  listRecent(refresh = false) {
    return invoke<LocatedFile[]>('list_recent', { refresh })
  },

  listShared(refresh = false) {
    return invoke<LocatedFile[]>('list_shared', { refresh })
  },

  listTrash(refresh = false) {
    return invoke<TrashList>('list_trash', { refresh })
  },

  restoreTrash(files: TrashItem[]) {
    return invoke<void>('restore_trash', {
      files: files.map(({ mountId, path }) => ({ mountId, path })),
    })
  },

  restoreAllTrash() {
    return invoke<void>('restore_trash', { files: [] })
  },

  emptyTrash(confirmation: string) {
    return invoke<void>('empty_trash', { confirmation })
  },

  selectUploadFile() {
    return invoke<LocalFileSelection | null>('select_upload_file')
  },

  selectDownloadLocation(suggestedName: string) {
    return invoke<LocalFileSelection | null>('select_download_location', { suggestedName })
  },

  selectDownloadFolder(suggestedName: string) {
    return invoke<LocalFileSelection | null>('select_download_folder', { suggestedName })
  },

  selectDownloadDirectory() {
    return invoke<string | null>('select_download_directory')
  },

  selectSyncFolder() {
    return invoke<LocalFileSelection | null>('select_sync_folder')
  },

  listSyncPairs() {
    return invoke<SyncPairSummary[]>('list_sync_pairs')
  },

  createSyncPair(localPathGrant: string, mountId: string, remotePath: string) {
    return invoke<SyncPairSummary>('create_sync_pair', {
      localPathGrant,
      mountId,
      remotePath,
    })
  },

  nativeSyncCapability() {
    return invoke<NativeSyncCapability>('get_native_sync_capability')
  },

  suggestedOnDemandSyncRoot() {
    return invoke<string>('suggested_on_demand_sync_root')
  },

  createOnDemandSyncPair(
    localPathGrant: string,
    mountId: string,
    remotePath: string,
    rootKind: SyncRootKind,
  ) {
    return invoke<SyncPairSummary>('create_on_demand_sync_pair', {
      localPathGrant,
      mountId,
      remotePath,
      rootKind,
    })
  },

  migrateSyncPairToOnDemand(
    pairId: string,
    localPathGrant: string,
    rootKind: SyncRootKind,
  ) {
    return invoke<SyncPairSummary>('migrate_sync_pair_to_on_demand', {
      pairId,
      localPathGrant,
      rootKind,
    })
  },

  openSyncRoot(pairId: string) {
    return invoke<void>('open_sync_root', { pairId })
  },

  setSyncPairPaused(pairId: string, paused: boolean) {
    return invoke<boolean>('set_sync_pair_paused', { pairId, paused })
  },

  runSyncPairNow(pairId: string) {
    return invoke<boolean>('run_sync_pair_now', { pairId })
  },

  listSyncIssues(pairId: string) {
    return invoke<SyncIssue[]>('list_sync_issues', { pairId })
  },

  listSyncTasks(pairId: string) {
    return invoke<SyncTaskSummary[]>('list_sync_tasks', { pairId })
  },

  removeSyncPair(pairId: string) {
    return invoke<boolean>('remove_sync_pair', { pairId })
  },

  selectWorkDirectory() {
    return invoke<string | null>('select_work_directory')
  },

  prepareDownloadLocation(
    suggestedName: string,
    downloadDirectory: string,
    conflictStrategy?: DownloadConflictStrategy,
  ) {
    return invoke<LocalFileSelection>('prepare_download_location', {
      suggestedName,
      downloadDirectory,
      conflictStrategy,
    })
  },

  prepareDownloadFolder(
    suggestedName: string,
    downloadDirectory: string,
    conflictStrategy?: DownloadConflictStrategy,
  ) {
    return invoke<LocalFileSelection>('prepare_download_folder', {
      suggestedName,
      downloadDirectory,
      conflictStrategy,
    })
  },

  createFolder(mountId: string, parentPath: string, name: string) {
    return invoke<void>('create_folder', { mountId, parentPath, name })
  },

  renameEntry(mountId: string, path: string, newName: string) {
    return invoke<void>('rename_entry', { mountId, path, newName })
  },

  moveEntry(
    mountId: string,
    path: string,
    destinationMountId: string,
    destinationDirectory: string,
  ) {
    return invoke<void>('move_entry', {
      mountId,
      path,
      destinationMountId,
      destinationDirectory,
    })
  },

  copyEntry(
    mountId: string,
    path: string,
    destinationMountId: string,
    destinationDirectory: string,
  ) {
    return invoke<void>('copy_entry', {
      mountId,
      path,
      destinationMountId,
      destinationDirectory,
    })
  },

  deleteEntry(mountId: string, path: string) {
    return invoke<void>('delete_entry', { mountId, path })
  },

  listVaults() {
    return invoke<VaultSummary[]>('list_vaults')
  },

  unlockVault(repoId: string, safeKey: string) {
    return invoke<VaultSummary>('unlock_vault', { repoId, safeKey })
  },

  lockVault(repoId: string) {
    return invoke<VaultSummary[]>('lock_vault', { repoId })
  },

  listVaultFiles(repoId: string, directoryId = 'root') {
    return invoke<VaultDirectory>('list_vault_files', { repoId, directoryId })
  },

  createVaultFolder(repoId: string, parentId: string, name: string) {
    return invoke<void>('create_vault_folder', { repoId, parentId, name })
  },

  renameVaultEntry(repoId: string, entryId: string, newName: string) {
    return invoke<void>('rename_vault_entry', { repoId, entryId, newName })
  },

  relocateVaultEntry(repoId: string, entryId: string, destinationId: string, isMove: boolean) {
    return invoke<void>('relocate_vault_entry', {
      repoId,
      entryId,
      destinationId,
      isMove,
    })
  },

  deleteVaultEntries(repoId: string, entryIds: string[]) {
    return invoke<void>('delete_vault_entries', { repoId, entryIds })
  },

  createVault(mountId: string, parentPath: string, name: string, safeKey: string) {
    return invoke<VaultSummary>('create_vault', { mountId, parentPath, name, safeKey })
  },

  removeVault(repoId: string, confirmation: string, safeKey: string) {
    return invoke<VaultSummary[]>('remove_vault', { repoId, confirmation, safeKey })
  },

  exportVaultRcloneConfig(repoId: string, safeKey: string) {
    return invoke<boolean>('export_vault_rclone_config', { repoId, safeKey })
  },

  importVaultRcloneConfig() {
    return invoke<VaultSummary[]>('import_vault_rclone_config')
  },

  uploadVaultFile(repoId: string, parentId: string, localPathGrant: string) {
    const transferId = crypto.randomUUID()
    return {
      transferId,
      result: invoke<TransferResult>('upload_vault_file', {
        transferId,
        repoId,
        parentId,
        localPathGrant,
      }),
    }
  },

  downloadVaultFile(
    repoId: string,
    entryId: string,
    displayName: string,
    localPathGrant: string,
  ) {
    const transferId = crypto.randomUUID()
    return {
      transferId,
      result: invoke<TransferResult>('download_vault_file', {
        transferId,
        repoId,
        entryId,
        displayName,
        localPathGrant,
      }),
    }
  },

  uploadFile(mountId: string, remoteDirectory: string, localPathGrant: string) {
    const transferId = crypto.randomUUID()
    return {
      transferId,
      result: invoke<TransferResult>('upload_file', {
        transferId,
        mountId,
        remoteDirectory,
        localPathGrant,
      }),
    }
  },

  uploadSplitFile(
    mountId: string,
    remoteDirectory: string,
    localPathGrant: string,
    settings: SplitUploadSettings,
  ) {
    const transferId = crypto.randomUUID()
    return {
      transferId,
      result: invoke<TransferResult>('upload_split_file', {
        request: {
          transferId,
          mountId,
          remoteDirectory,
          localPathGrant,
          packageName: settings.packageName,
          partBytes: settings.partBytes,
        },
      }),
    }
  },

  downloadFile(mountId: string, remotePath: string, localPathGrant: string) {
    const transferId = crypto.randomUUID()
    return {
      transferId,
      result: invoke<TransferResult>('download_file', {
        transferId,
        mountId,
        remotePath,
        localPathGrant,
      }),
    }
  },

  downloadFolder(mountId: string, remotePath: string, localPathGrant: string) {
    const transferId = crypto.randomUUID()
    return {
      transferId,
      result: invoke<TransferResult>('download_folder', {
        transferId,
        mountId,
        remotePath,
        localPathGrant,
      }),
    }
  },

  cancelTransfer(transferId: string) {
    return invoke<boolean>('cancel_transfer', { transferId })
  },

  pauseTransfer(transferId: string) {
    return invoke<boolean>('pause_transfer', { transferId })
  },

  listResumableTransfers() {
    return invoke<ResumableTransfer[]>('list_resumable_transfers')
  },

  listDownloadHistory() {
    return invoke<DownloadHistoryItem[]>('list_download_history')
  },

  clearFinishedDownloadHistory() {
    return invoke<number>('clear_finished_download_history')
  },

  resumeTransfer(transferId: string) {
    return invoke<TransferResult>('resume_transfer', { transferId })
  },

  discardResumableTransfer(transferId: string) {
    return invoke<boolean>('discard_resumable_transfer', { transferId })
  },

  openDownloadedFile(transferId: string) {
    return invoke<void>('open_downloaded_file', { transferId })
  },

  openDownloadedFolder(transferId: string) {
    return invoke<void>('open_downloaded_folder', { transferId })
  },

  prepareFilePreview(mountId: string, remotePath: string) {
    return invoke<FilePreviewInfo>('prepare_file_preview', { mountId, remotePath })
  },

  prepareVaultPreview(repoId: string, entryId: string) {
    return invoke<FilePreviewInfo>('prepare_vault_preview', { repoId, entryId })
  },

  readPreviewText(sessionId: string, encoding?: string) {
    return invoke<PreviewTextResult>('read_preview_text', { sessionId, encoding })
  },

  closeFilePreview(sessionId: string) {
    return invoke<void>('close_file_preview', { sessionId })
  },

  filePreviewUrl(sessionId: string) {
    return `http://koofr-stream.localhost/${sessionId}`
  },

  onTransferProgress(listener: (progress: TransferProgress) => void): Promise<UnlistenFn> {
    return listen<TransferProgress>(TRANSFER_EVENT, (event) => listener(event.payload))
  },

  onSyncState(listener: (state: SyncStateEvent) => void): Promise<UnlistenFn> {
    return listen<SyncStateEvent>(SYNC_STATE_EVENT, (event) => listener(event.payload))
  },

  publishFloatingTransferSnapshot(snapshot: FloatingTransferSnapshot) {
    return emitTo('transfer-float', FLOATING_TRANSFER_EVENT, snapshot)
  },

  onFloatingTransferSnapshot(
    listener: (snapshot: FloatingTransferSnapshot) => void,
  ): Promise<UnlistenFn> {
    return listen<FloatingTransferSnapshot>(
      FLOATING_TRANSFER_EVENT,
      (event) => listener(event.payload),
    )
  },

  onFloatingWindowDisabled(listener: () => void): Promise<UnlistenFn> {
    return listen(FLOATING_WINDOW_DISABLED_EVENT, listener)
  },

  emitFloatingTransferAction(action: FloatingTransferAction) {
    return emitTo('main', FLOATING_TRANSFER_ACTION_EVENT, { action })
  },

  onFloatingTransferAction(
    listener: (action: FloatingTransferAction) => void,
  ): Promise<UnlistenFn> {
    return listen<{ action: FloatingTransferAction }>(
      FLOATING_TRANSFER_ACTION_EVENT,
      (event) => listener(event.payload.action),
    )
  },

  setFloatingWindowCompact(compact: boolean, reduceMotion: boolean) {
    return invoke<void>('set_floating_window_compact', { compact, reduceMotion })
  },

  startFloatingWindowDrag() {
    return getCurrentWindow().startDragging()
  },

  showFloatingContextMenu() {
    return invoke<void>('show_floating_context_menu')
  },

  openMainWindowFromFloating() {
    return invoke<void>('open_main_window_from_floating')
  },

  runWindowCommand(command: 'minimize' | 'toggleMaximize' | 'close') {
    if (!isTauriRuntime()) return
    void getCurrentWindow()[command]()
  },
}
