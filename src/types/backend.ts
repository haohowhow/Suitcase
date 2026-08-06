export interface CommandError {
  code:
    | 'authentication_failed'
    | 'not_authenticated'
    | 'account_identity_unavailable'
    | 'invalid_input'
    | 'conflict'
    | 'not_found'
    | 'forbidden'
    | 'cancelled'
    | 'transfer_paused'
    | 'incomplete_transfer'
    | 'integrity_error'
    | 'remote_error'
    | 'network_error'
    | 'local_io_error'
    | 'invalid_response'
    | 'initialization_error'
    | 'dialog_error'
    | 'local_open_error'
    | 'local_data_error'
    | 'credential_store_error'
    | 'vault_locked'
    | 'vault_invalid_key'
    | 'vault_crypto_error'
    | 'vault_prompt_error'
  message: string
  diagnostic?: string
}

export interface KoofrSession {
  authenticated: boolean
  userId: string | null
}

export interface LoginBootstrap {
  session: KoofrSession
  savedEmail: string | null
}

export type CacheMode = 'off' | 'memory' | 'disk'
export type LogLevel = 'error' | 'warn' | 'info' | 'debug'
export type InterfaceLanguage = 'system' | 'zhCn' | 'enUs'
export type UiDensity = 'standard' | 'compact'
export type DownloadConflictStrategy = 'ask' | 'skip' | 'rename'
export type BandwidthMode = 'full' | 'limited' | 'idleFull'
export type LinkDefaultKind = 'download' | 'upload'

export interface AppSettings {
  readonly cacheMode: CacheMode
  readonly cacheTtlMinutes: number
  readonly cachedItems: number
  readonly cacheDiskBytes: number
  readonly savedEmail: string | null
  readonly downloadDirectory: string
  readonly askDownloadLocation: boolean
  readonly workDirectory: string
  readonly pendingWorkDirectory: string | null
  readonly pendingWorkDirectoryMove: boolean
  readonly workDirectoryMigrationFailed: boolean
  readonly logLevel: LogLevel
  readonly logRetentionDays: number
  readonly logMaxFileSizeMb: number
  readonly logFiles: number
  readonly logDiskBytes: number
  readonly autoRetryNetworkErrors: boolean
  readonly networkRetryLimit: number | null
  readonly networkRetryIntervalSeconds: number
  readonly language: InterfaceLanguage
  readonly reduceMotion: boolean
  readonly density: UiDensity
  readonly launchAtLogin: boolean
  readonly closeToTray: boolean
  readonly minimizeToTray: boolean
  readonly confirmQuitWithTransfers: boolean
  readonly uploadConcurrency: number
  readonly downloadConcurrency: number
  readonly defaultSplitPartMib: number
  readonly autoSplitThresholdMib: number
  readonly downloadConflictStrategy: DownloadConflictStrategy
  readonly openFolderAfterDownload: boolean
  readonly verifyHashAfterDownload: boolean
  readonly keepFailedPartDays: number
  readonly globalBandwidthLimitKbps: number
  readonly perTaskBandwidthLimitKbps: number
  readonly bandwidthMode: BandwidthMode
  readonly notifyOnComplete: boolean
  readonly notifySound: boolean
  readonly trayBadge: boolean
  readonly floatingWindowEnabled: boolean
  readonly floatingWindowPrompted: boolean
  readonly floatingWindowCompactEnabled: boolean
  readonly floatingWindowClickToggleEnabled: boolean
  readonly floatingWindowDoubleClickOpenEnabled: boolean
  readonly linkDefaultKind: LinkDefaultKind
  readonly linkDefaultExpireDays: number
  readonly linkDefaultRequirePassword: boolean
  readonly idleAutoLogoutMinutes: number
  readonly downloadHistoryKeepDays: number
  readonly downloadHistoryKeepCount: number
}

export interface AppVersionInfo {
  readonly version: string
  readonly homepageUrl: string
  readonly releasesUrl: string
  readonly issuesUrl: string
  readonly license: string
}

export interface KoofrMount {
  id: string
  name: string
  mountType: string
  spaceTotal: number
  spaceUsed: number
  online: boolean
  isPrimary: boolean
  isShared: boolean
}

export type PublicLinkKind = 'download' | 'upload'

export interface PublicLink {
  readonly id: string
  readonly name: string
  readonly path: string
  readonly counter: number
  readonly url: string
  readonly shortUrl: string
  readonly hasPassword: boolean
  readonly kind: PublicLinkKind
  readonly generatedPassword?: string
}

export interface CreatePublicLinkOptions {
  readonly kind: PublicLinkKind
  readonly expireDays: number
  readonly requirePassword: boolean
}

export interface RemoteFile {
  name: string
  entryType: string
  modified: number
  size: number
  contentType: string
  hash: string
  path: string
}

export interface LocatedFile extends RemoteFile {
  mountId: string
  mountName: string
  shareDirection: 'outgoing' | 'received' | null
}

export interface TrashItem {
  versionId: string
  mountId: string
  mountName: string
  path: string
  name: string
  deleted: string
  size: number
  contentType: string
}

export interface TrashList {
  items: TrashItem[]
  retentionDays: number
}

export interface TrashRestoreTarget {
  mountId: string
  path: string
}

export type TransferDirection = 'upload' | 'download'
export type TransferState = 'queued' | 'running' | 'retrying' | 'paused' | 'completed' | 'cancelled' | 'failed'
export type IntegrityStatus = 'notRequested' | 'unavailable' | 'verified'
export type RecoveryKind = 'byte_resume' | 'chunk_resume' | 'restart'

export interface SplitProgress {
  readonly completedParts: number
  readonly currentPart: number | null
  readonly totalParts: number
  readonly phase: 'parts' | 'finalizing'
}

export interface TransferProgress {
  transferId: string
  direction: TransferDirection
  state: TransferState
  bytesTransferred: number
  totalBytes: number | null
  splitProgress?: SplitProgress
}

export interface DownloadSpeedSample {
  readonly recordedAt: number
  readonly bytesTransferred: number
}

export interface DownloadHistoryItem {
  readonly transferId: string
  readonly name: string
  readonly state: TransferState
  readonly bytesTransferred: number
  readonly totalBytes: number | null
  readonly localKind: 'file' | 'folder'
  readonly recoveryKind: RecoveryKind | null
  readonly remotePath: string
  readonly localPath: string
  readonly startedAt: number
  readonly finishedAt: number | null
  readonly speedSamples: DownloadSpeedSample[]
  readonly integrityStatus: IntegrityStatus
}

export interface TransferResult {
  transferId: string
  bytesTransferred: number
  file: RemoteFile | null
  integrityStatus: IntegrityStatus
}

export interface ResumableTransfer {
  readonly transferId: string
  readonly name: string
  readonly direction: TransferDirection
  readonly recoveryKind: RecoveryKind
  readonly bytesTransferred: number
  readonly totalBytes: number
  readonly splitProgress?: SplitProgress
}

export interface LocalFileSelection {
  readonly grantId: string
  readonly fileName: string
  readonly localPath: string | null
  readonly fileSize: number | null
}

export type SyncPairStatus =
  | 'initializing'
  | 'scanning'
  | 'syncing'
  | 'idle'
  | 'paused'
  | 'offline'
  | 'attention'

export interface SyncEndpointStats {
  readonly fileCount: number
  readonly directoryCount: number
  readonly bytes: number
}

export interface SyncDirectionStats {
  readonly pendingItems: number
  readonly totalItems: number
  readonly completedItems: number
  readonly pendingBytes: number
  readonly totalBytes: number
  readonly completedBytes: number
  readonly bytesPerSecond: number
}

export interface SyncPairSummary {
  readonly id: string
  readonly localRoot: string
  readonly remotePath: string
  readonly status: SyncPairStatus
  readonly paused: boolean
  readonly pendingItems: number
  readonly pendingBytes: number
  readonly completedItems: number
  readonly totalItems: number
  readonly completedBytes: number
  readonly totalBytes: number
  readonly lastSyncedAt: number | null
  readonly issueCount: number
  readonly lastErrorCode: string | null
  readonly local: SyncEndpointStats
  readonly remote: SyncEndpointStats
  readonly upload: SyncDirectionStats
  readonly download: SyncDirectionStats
  readonly lastScanAt: number | null
}

export interface SyncIssue {
  readonly id: number
  readonly pairId: string
  readonly relativePath: string
  readonly code: string
  readonly message: string
  readonly createdAt: number
}

export interface SyncStateEvent {
  readonly pairId: string
  readonly status: SyncPairStatus
  readonly pendingItems: number
  readonly pendingBytes: number
  readonly completedItems: number
  readonly totalItems: number
  readonly completedBytes: number
  readonly totalBytes: number
  readonly lastSyncedAt: number | null
  readonly issueCount: number
  readonly errorCode: string | null
  readonly local: SyncEndpointStats
  readonly remote: SyncEndpointStats
  readonly upload: SyncDirectionStats
  readonly download: SyncDirectionStats
  readonly lastScanAt: number | null
}

export interface VaultSummary {
  readonly id: string
  readonly name: string
  readonly locked: boolean
  readonly added: number
  readonly autoLockSeconds: number
}

export interface VaultBreadcrumb {
  readonly id: string
  readonly name: string
}

export interface VaultEntry {
  readonly id: string
  readonly name: string
  readonly entryType: 'file' | 'dir'
  readonly modified: number
  readonly size: number
  readonly contentType: string
}

export interface VaultDirectory {
  readonly repoId: string
  readonly repoName: string
  readonly directoryId: string
  readonly breadcrumbs: VaultBreadcrumb[]
  readonly entries: VaultEntry[]
}

export type FilePreviewKind = 'video' | 'audio' | 'image' | 'pdf' | 'text'

export interface FilePreviewInfo {
  readonly sessionId: string
  readonly fileSize: number
  readonly contentType: string
  readonly kind: FilePreviewKind
  readonly decoder: 'native' | 'pdfjs' | 'text' | 'heic' | 'svg'
  readonly limits: {
    readonly maxBytes: number | null
    readonly textBytes: number | null
    readonly maxPixels: number | null
  }
}

export interface PreviewTextResult {
  readonly text: string
  readonly encoding: string
  readonly truncated: boolean
  readonly hadDecodeErrors: boolean
}
