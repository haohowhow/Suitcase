import {
  Ban,
  Bell,
  Bug,
  Cog,
  Database,
  Download,
  FolderOpen,
  Gauge,
  HardDrive,
  History,
  Info,
  KeyRound,
  Link as LinkIcon,
  MemoryStick,
  Palette,
  RefreshCw,
  ShieldCheck,
  Trash2,
} from 'lucide-react'
import { memo, useEffect, useMemo, useState, type ComponentType } from 'react'
import { themes, type ThemeId } from '../../theme'
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
} from '../../types/backend'
import { formatBytes } from '../files/filePresentation'

interface SettingsPanelProps {
  readonly settings: AppSettings | null
  readonly loading: boolean
  readonly busy: boolean
  readonly error: string
  readonly downloadError: string
  readonly onCacheModeChange: (mode: CacheMode) => void
  readonly onCacheTtlChange: (minutes: number) => void
  readonly onLoggingSettingsChange: (settings: {
    logLevel: LogLevel
    logRetentionDays: number
    logMaxFileSizeMb: number
  }) => void
  readonly onTransferSettingsChange: (settings: {
    autoRetryNetworkErrors: boolean
    networkRetryLimit: number | null
    networkRetryIntervalSeconds: number
  }) => void
  readonly onDownloadSettingsChange: (directory: string, askDownloadLocation: boolean) => void
  readonly onBrowseDownloadDirectory: () => Promise<string | null>
  readonly onBrowseWorkDirectory: () => Promise<string | null>
  readonly onWorkDirectoryChange: (directory: string, moveExisting: boolean) => void
  readonly onClearCache: () => void
  readonly onClearLogs: () => void
  readonly onForgetLogin: () => void
  // New optional handlers (fall back to no-op when omitted, e.g. in unit tests).
  readonly themeId?: ThemeId
  readonly onThemeChange?: (theme: ThemeId) => void
  readonly onAppearanceChange?: (next: {
    language: InterfaceLanguage
    reduceMotion: boolean
    density: UiDensity
  }) => void
  readonly onGeneralChange?: (next: {
    launchAtLogin: boolean
    closeToTray: boolean
    minimizeToTray: boolean
    confirmQuitWithTransfers: boolean
  }) => void
  readonly onTransferAdvancedChange?: (next: {
    uploadConcurrency: number
    downloadConcurrency: number
    defaultSplitPartMib: number
    autoSplitThresholdMib: number
  }) => void
  readonly onDownloadBehaviorChange?: (next: {
    conflictStrategy: DownloadConflictStrategy
    openFolderAfterDownload: boolean
    verifyHashAfterDownload: boolean
    keepFailedPartDays: number
  }) => void
  readonly onBandwidthChange?: (next: {
    globalKbps: number
    perTaskKbps: number
    mode: BandwidthMode
  }) => void
  readonly onNotificationChange?: (next: {
    notifyOnComplete: boolean
    notifySound: boolean
    trayBadge: boolean
    floatingWindowEnabled: boolean
    floatingWindowPrompted: boolean
    floatingWindowCompactEnabled: boolean
    floatingWindowClickToggleEnabled: boolean
    floatingWindowDoubleClickOpenEnabled: boolean
  }) => void
  readonly onLinkDefaultsChange?: (next: {
    kind: LinkDefaultKind
    expireDays: number
    requirePassword: boolean
  }) => void
  readonly onSecurityChange?: (next: {
    idleAutoLogoutMinutes: number
  }) => void
  readonly onHistoryPolicyChange?: (next: { keepDays: number; keepCount: number }) => void
  readonly onClearDownloadHistory?: () => void
  readonly versionInfo?: AppVersionInfo | null
  readonly onOpenExternalPage?: (page: 'homepage' | 'releases' | 'issues') => void
}

type SectionId =
  | 'appearance'
  | 'general'
  | 'download'
  | 'transfer'
  | 'cache'
  | 'logs'
  | 'links'
  | 'notifications'
  | 'security'
  | 'history'
  | 'about'

interface SectionMeta {
  readonly id: SectionId
  readonly label: string
  readonly icon: ComponentType<{ 'aria-hidden'?: boolean | 'true' | 'false' }>
}

const SECTIONS: readonly SectionMeta[] = [
  { id: 'appearance', label: '外观', icon: Palette },
  { id: 'general', label: '通用', icon: Cog },
  { id: 'download', label: '下载', icon: Download },
  { id: 'transfer', label: '传输', icon: RefreshCw },
  { id: 'cache', label: '缓存', icon: Database },
  { id: 'logs', label: '诊断日志', icon: Bug },
  { id: 'links', label: '分享链接', icon: LinkIcon },
  { id: 'notifications', label: '通知', icon: Bell },
  { id: 'security', label: '隐私与安全', icon: ShieldCheck },
  { id: 'history', label: '历史', icon: History },
  { id: 'about', label: '关于', icon: Info },
]

const CACHE_OPTIONS = [
  {
    id: 'off',
    title: '不缓存',
    description: '每次打开目录都从 Koofr 获取最新数据。',
    icon: Ban,
  },
  {
    id: 'memory',
    title: '仅内存',
    description: '本次运行期间复用数据，关闭应用后自动清除。',
    icon: MemoryStick,
  },
  {
    id: 'disk',
    title: '本地磁盘',
    description: '重启应用后仍可复用文件元数据。',
    icon: HardDrive,
  },
] satisfies Array<{
  id: CacheMode
  title: string
  description: string
  icon: typeof Ban
}>

export const SettingsPanel = memo(function SettingsPanel(props: SettingsPanelProps) {
  const {
    settings,
    loading,
    busy,
    error,
    downloadError,
    onCacheModeChange,
    onCacheTtlChange,
    onLoggingSettingsChange,
    onTransferSettingsChange,
    onDownloadSettingsChange,
    onBrowseDownloadDirectory,
    onBrowseWorkDirectory,
    onWorkDirectoryChange,
    onClearCache,
    onClearLogs,
    onForgetLogin,
    themeId,
    onThemeChange,
    onAppearanceChange,
    onGeneralChange,
    onTransferAdvancedChange,
    onDownloadBehaviorChange,
    onBandwidthChange,
    onNotificationChange,
    onLinkDefaultsChange,
    onSecurityChange,
    onHistoryPolicyChange,
    onClearDownloadHistory,
    versionInfo,
    onOpenExternalPage,
  } = props

  const [activeSection, setActiveSection] = useState<SectionId>('appearance')
  const [downloadDirectory, setDownloadDirectory] = useState(settings?.downloadDirectory ?? '')
  const [workDirectory, setWorkDirectory] = useState(
    settings?.pendingWorkDirectory ?? settings?.workDirectory ?? '',
  )
  const [moveExisting, setMoveExisting] = useState(settings?.pendingWorkDirectoryMove ?? true)
  const [logLevel, setLogLevel] = useState<LogLevel>(settings?.logLevel ?? 'info')
  const [logRetentionDays, setLogRetentionDays] = useState(settings?.logRetentionDays ?? 14)
  const [logMaxFileSizeMb, setLogMaxFileSizeMb] = useState(settings?.logMaxFileSizeMb ?? 10)
  const [browsing, setBrowsing] = useState<'download' | 'work' | null>(null)

  useEffect(() => {
    if (!settings) return
    setDownloadDirectory(settings.downloadDirectory)
    setWorkDirectory(settings.pendingWorkDirectory ?? settings.workDirectory)
    setMoveExisting(settings.pendingWorkDirectoryMove || settings.pendingWorkDirectory === null)
    setLogLevel(settings.logLevel)
    setLogRetentionDays(settings.logRetentionDays)
    setLogMaxFileSizeMb(settings.logMaxFileSizeMb)
  }, [settings])

  const cacheDetails = useMemo(
    () => (settings ? { items: settings.cachedItems, bytes: settings.cacheDiskBytes } : null),
    [settings],
  )

  if (loading || !settings) {
    return (
      <div className="settings-loading">
        <span className="auth-spinner" />
        正在读取本地设置…
      </div>
    )
  }

  const normalizedDownloadDirectory = downloadDirectory.trim()
  const normalizedWorkDirectory = workDirectory.trim()
  const downloadDirectoryChanged = normalizedDownloadDirectory !== settings.downloadDirectory
  const configuredWorkDirectory = settings.pendingWorkDirectory ?? settings.workDirectory
  const workDirectoryChanged =
    normalizedWorkDirectory !== configuredWorkDirectory
    || (settings.pendingWorkDirectory !== null && moveExisting !== settings.pendingWorkDirectoryMove)
  const loggingChanged =
    logLevel !== settings.logLevel
    || logRetentionDays !== settings.logRetentionDays
    || logMaxFileSizeMb !== settings.logMaxFileSizeMb

  const browseDownloadDirectory = async () => {
    setBrowsing('download')
    try {
      const selected = await onBrowseDownloadDirectory()
      if (selected) {
        setDownloadDirectory(selected)
        onDownloadSettingsChange(selected, settings.askDownloadLocation)
      }
    } finally {
      setBrowsing(null)
    }
  }
  const browseWorkDirectory = async () => {
    setBrowsing('work')
    try {
      const selected = await onBrowseWorkDirectory()
      if (selected) setWorkDirectory(selected)
    } finally {
      setBrowsing(null)
    }
  }

  const updateAppearance = (patch: Partial<{
    language: InterfaceLanguage
    reduceMotion: boolean
    density: UiDensity
  }>) => {
    if (!onAppearanceChange) return
    onAppearanceChange({
      language: patch.language ?? settings.language,
      reduceMotion: patch.reduceMotion ?? settings.reduceMotion,
      density: patch.density ?? settings.density,
    })
  }

  const updateGeneral = (patch: Partial<{
    launchAtLogin: boolean
    closeToTray: boolean
    minimizeToTray: boolean
    confirmQuitWithTransfers: boolean
  }>) => {
    if (!onGeneralChange) return
    onGeneralChange({
      launchAtLogin: patch.launchAtLogin ?? settings.launchAtLogin,
      closeToTray: patch.closeToTray ?? settings.closeToTray,
      minimizeToTray: patch.minimizeToTray ?? settings.minimizeToTray,
      confirmQuitWithTransfers:
        patch.confirmQuitWithTransfers ?? settings.confirmQuitWithTransfers,
    })
  }

  const updateAdvanced = (patch: Partial<{
    uploadConcurrency: number
    downloadConcurrency: number
    defaultSplitPartMib: number
    autoSplitThresholdMib: number
  }>) => {
    if (!onTransferAdvancedChange) return
    onTransferAdvancedChange({
      uploadConcurrency: patch.uploadConcurrency ?? settings.uploadConcurrency,
      downloadConcurrency: patch.downloadConcurrency ?? settings.downloadConcurrency,
      defaultSplitPartMib: patch.defaultSplitPartMib ?? settings.defaultSplitPartMib,
      autoSplitThresholdMib: patch.autoSplitThresholdMib ?? settings.autoSplitThresholdMib,
    })
  }

  const updateBehavior = (patch: Partial<{
    conflictStrategy: DownloadConflictStrategy
    openFolderAfterDownload: boolean
    verifyHashAfterDownload: boolean
    keepFailedPartDays: number
  }>) => {
    if (!onDownloadBehaviorChange) return
    onDownloadBehaviorChange({
      conflictStrategy: patch.conflictStrategy ?? settings.downloadConflictStrategy,
      openFolderAfterDownload:
        patch.openFolderAfterDownload ?? settings.openFolderAfterDownload,
      verifyHashAfterDownload:
        patch.verifyHashAfterDownload ?? settings.verifyHashAfterDownload,
      keepFailedPartDays: patch.keepFailedPartDays ?? settings.keepFailedPartDays,
    })
  }

  const updateBandwidth = (patch: Partial<{
    globalKbps: number
    perTaskKbps: number
    mode: BandwidthMode
  }>) => {
    if (!onBandwidthChange) return
    onBandwidthChange({
      globalKbps: patch.globalKbps ?? settings.globalBandwidthLimitKbps,
      perTaskKbps: patch.perTaskKbps ?? settings.perTaskBandwidthLimitKbps,
      mode: patch.mode ?? settings.bandwidthMode,
    })
  }

  const updateNotifications = (patch: Partial<{
    notifyOnComplete: boolean
    notifySound: boolean
    trayBadge: boolean
    floatingWindowEnabled: boolean
    floatingWindowPrompted: boolean
    floatingWindowCompactEnabled: boolean
    floatingWindowClickToggleEnabled: boolean
    floatingWindowDoubleClickOpenEnabled: boolean
  }>) => {
    if (!onNotificationChange) return
    onNotificationChange({
      notifyOnComplete: patch.notifyOnComplete ?? settings.notifyOnComplete,
      notifySound: patch.notifySound ?? settings.notifySound,
      trayBadge: patch.trayBadge ?? settings.trayBadge,
      floatingWindowEnabled:
        patch.floatingWindowEnabled ?? settings.floatingWindowEnabled,
      floatingWindowPrompted:
        patch.floatingWindowPrompted ?? settings.floatingWindowPrompted,
      floatingWindowCompactEnabled:
        patch.floatingWindowCompactEnabled ?? settings.floatingWindowCompactEnabled,
      floatingWindowClickToggleEnabled:
        patch.floatingWindowClickToggleEnabled ?? settings.floatingWindowClickToggleEnabled,
      floatingWindowDoubleClickOpenEnabled:
        patch.floatingWindowDoubleClickOpenEnabled ?? settings.floatingWindowDoubleClickOpenEnabled,
    })
  }

  const updateLinks = (patch: Partial<{
    kind: LinkDefaultKind
    expireDays: number
    requirePassword: boolean
  }>) => {
    if (!onLinkDefaultsChange) return
    onLinkDefaultsChange({
      kind: patch.kind ?? settings.linkDefaultKind,
      expireDays: patch.expireDays ?? settings.linkDefaultExpireDays,
      requirePassword: patch.requirePassword ?? settings.linkDefaultRequirePassword,
    })
  }

  const updateSecurity = (patch: Partial<{
    idleAutoLogoutMinutes: number
  }>) => {
    if (!onSecurityChange) return
    onSecurityChange({
      idleAutoLogoutMinutes:
        patch.idleAutoLogoutMinutes ?? settings.idleAutoLogoutMinutes,
    })
  }

  const updateHistory = (patch: Partial<{ keepDays: number; keepCount: number }>) => {
    if (!onHistoryPolicyChange) return
    onHistoryPolicyChange({
      keepDays: patch.keepDays ?? settings.downloadHistoryKeepDays,
      keepCount: patch.keepCount ?? settings.downloadHistoryKeepCount,
    })
  }

  return (
    <div className="settings-panel settings-panel--nav">
      <nav className="settings-nav" role="tablist" aria-label="设置分组">
        {SECTIONS.map((section) => {
          const Icon = section.icon
          const selected = section.id === activeSection
          return (
            <button
              key={section.id}
              className={`settings-nav__item${selected ? ' settings-nav__item--selected' : ''}`}
              type="button"
              role="tab"
              aria-selected={selected}
              onClick={() => setActiveSection(section.id)}
            >
              <Icon aria-hidden="true" />
              <span>{section.label}</span>
            </button>
          )
        })}
      </nav>

      <div className="settings-content" role="tabpanel" aria-label={sectionLabel(activeSection)}>
        {activeSection === 'appearance' ? (
          <section className="settings-section">
            <header className="settings-section__heading">
              <Palette aria-hidden="true" />
              <div>
                <h3>外观</h3>
                <p>主题色、界面密度和动效偏好，仅保存在本机。</p>
              </div>
            </header>

            {themeId && onThemeChange ? (
              <div className="settings-row">
                <label>
                  <strong>强调色主题</strong>
                  <small>选择应用界面的主强调色</small>
                </label>
                <div className="settings-theme-swatches" role="radiogroup" aria-label="强调色">
                  {themes.map((theme) => {
                    const selected = theme.id === themeId
                    return (
                      <button
                        key={theme.id}
                        type="button"
                        role="radio"
                        aria-checked={selected}
                        aria-label={theme.label}
                        title={theme.label}
                        disabled={busy}
                        className={`settings-theme-swatch${selected ? ' settings-theme-swatch--selected' : ''}`}
                        style={{ background: theme.accent }}
                        onClick={() => onThemeChange(theme.id)}
                      />
                    )
                  })}
                </div>
              </div>
            ) : null}

            <div className="settings-row">
              <label htmlFor="settings-language">
                <strong>界面语言</strong>
                <small>切换后立即刷新应用界面</small>
              </label>
              <select
                id="settings-language"
                value={settings.language}
                disabled={busy || !onAppearanceChange}
                onChange={(event) =>
                  updateAppearance({ language: event.target.value as InterfaceLanguage })}
              >
                <option value="system">跟随系统</option>
                <option value="zhCn">简体中文</option>
                <option value="enUs">English</option>
              </select>
            </div>

            <div className="settings-row">
              <label htmlFor="settings-density">
                <strong>界面密度</strong>
                <small>紧凑模式在列表中显示更多内容</small>
              </label>
              <select
                id="settings-density"
                value={settings.density}
                disabled={busy || !onAppearanceChange}
                onChange={(event) => updateAppearance({ density: event.target.value as UiDensity })}
              >
                <option value="standard">标准</option>
                <option value="compact">紧凑</option>
              </select>
            </div>

            <div className="settings-switch-row">
              <span>
                <strong>减少动效</strong>
                <small>关闭平滑过渡与非必要动画</small>
              </span>
              <button
                className={`settings-switch${settings.reduceMotion ? ' settings-switch--on' : ''}`}
                type="button"
                role="switch"
                aria-checked={settings.reduceMotion}
                aria-label="减少动效"
                disabled={busy || !onAppearanceChange}
                onClick={() => updateAppearance({ reduceMotion: !settings.reduceMotion })}
              >
                <span aria-hidden="true" />
              </button>
            </div>

            <SwitchRow
              label="显示传输悬浮窗"
              hint="置顶显示上传、下载或暂停状态；单击可打开传输面板"
              checked={settings.floatingWindowEnabled}
              disabled={busy || !onNotificationChange}
              onChange={(next) => updateNotifications({
                floatingWindowEnabled: next,
                floatingWindowPrompted: true,
              })}
            />
            {settings.floatingWindowEnabled ? (
              <>
                <SwitchRow
                  label="空闲或暂停时收为图标"
                  hint="任务完成提示结束后，悬浮窗收为单图标方块"
                  checked={settings.floatingWindowCompactEnabled}
                  disabled={busy || !onNotificationChange}
                  onChange={(next) => updateNotifications({ floatingWindowCompactEnabled: next })}
                />
                <SwitchRow
                  label="单击暂停或继续"
                  hint="对所有可操作传输执行暂停或继续"
                  checked={settings.floatingWindowClickToggleEnabled}
                  disabled={busy || !onNotificationChange}
                  onChange={(next) => updateNotifications({ floatingWindowClickToggleEnabled: next })}
                />
                <SwitchRow
                  label="双击打开应用"
                  hint="双击悬浮窗显示并聚焦 Koofr GUI"
                  checked={settings.floatingWindowDoubleClickOpenEnabled}
                  disabled={busy || !onNotificationChange}
                  onChange={(next) => updateNotifications({ floatingWindowDoubleClickOpenEnabled: next })}
                />
              </>
            ) : null}
          </section>
        ) : null}

        {activeSection === 'general' ? (
          <section className="settings-section">
            <header className="settings-section__heading">
              <Cog aria-hidden="true" />
              <div>
                <h3>通用</h3>
                <p>启动、托盘与退出行为。</p>
              </div>
            </header>

            <SwitchRow
              label="开机启动 Koofr GUI"
              hint={settings.nativeSyncBackgroundRequired ? '存在按需同步根时必需；登录 Windows 后自动恢复原生文件访问' : '登录 Windows 后自动启动，仅写入当前用户注册表'}
              checked={settings.launchAtLogin}
              disabled={busy || !onGeneralChange || settings.nativeSyncBackgroundRequired}
              onChange={(next) => updateGeneral({ launchAtLogin: next })}
            />
            <SwitchRow
              label="关闭窗口时最小化到托盘"
              hint={settings.nativeSyncBackgroundRequired ? '存在按需同步根时必需；仍可从托盘菜单明确完全退出' : '点击窗口关闭按钮时不退出应用'}
              checked={settings.closeToTray}
              disabled={busy || !onGeneralChange || settings.nativeSyncBackgroundRequired}
              onChange={(next) => updateGeneral({ closeToTray: next })}
            />
            <SwitchRow
              label="最小化时进入托盘"
              hint="点击最小化按钮时收起到任务栏托盘"
              checked={settings.minimizeToTray}
              disabled={busy || !onGeneralChange}
              onChange={(next) => updateGeneral({ minimizeToTray: next })}
            />
            <SwitchRow
              label="退出前若有传输在进行则询问"
              hint="避免误关闭导致未完成的上传/下载"
              checked={settings.confirmQuitWithTransfers}
              disabled={busy || !onGeneralChange}
              onChange={(next) => updateGeneral({ confirmQuitWithTransfers: next })}
            />

            <p className="settings-note">
              说明：托盘图标常驻 Windows 通知区域；若被系统折叠，可在任务栏设置中将其固定。
            </p>
          </section>
        ) : null}

        {activeSection === 'download' ? (
          <section className="settings-section settings-section--download">
            <header className="settings-section__heading">
              <Download aria-hidden="true" />
              <div>
                <h3>下载</h3>
                <p>默认保存位置、冲突策略与完成后行为。</p>
              </div>
            </header>

            <div className="settings-path-field">
              <label htmlFor="settings-download-directory">默认下载位置</label>
              <span className="path-field__control">
                <input
                  id="settings-download-directory"
                  value={downloadDirectory}
                  disabled={busy}
                  aria-invalid={downloadError ? true : undefined}
                  aria-describedby={
                    downloadError ? 'settings-download-error' : 'settings-download-hint'
                  }
                  onChange={(event) => setDownloadDirectory(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && normalizedDownloadDirectory) {
                      onDownloadSettingsChange(
                        normalizedDownloadDirectory,
                        settings.askDownloadLocation,
                      )
                    }
                  }}
                />
                <button
                  type="button"
                  aria-label="选择默认下载文件夹"
                  title="选择文件夹"
                  disabled={busy || browsing !== null}
                  onClick={() => void browseDownloadDirectory()}
                >
                  <FolderOpen aria-hidden="true" />
                </button>
              </span>
            </div>
            <div className="settings-path-actions">
              <small id="settings-download-hint">
                可直接填写完整路径，也可以使用右侧按钮选择。
              </small>
              <button
                type="button"
                aria-label="保存下载路径"
                disabled={busy || !downloadDirectoryChanged || !normalizedDownloadDirectory}
                onClick={() =>
                  onDownloadSettingsChange(
                    normalizedDownloadDirectory,
                    settings.askDownloadLocation,
                  )}
              >
                保存路径
              </button>
            </div>
            {downloadError ? (
              <p
                className="field-message field-message--error"
                id="settings-download-error"
                role="alert"
              >
                {downloadError}
              </p>
            ) : null}

            <div className="settings-switch-row">
              <span>
                <strong>每次下载前询问保存位置</strong>
                <small>关闭后将直接下载到上面的默认文件夹。</small>
              </span>
              <button
                className={`settings-switch${settings.askDownloadLocation ? ' settings-switch--on' : ''}`}
                type="button"
                role="switch"
                aria-checked={settings.askDownloadLocation}
                aria-label="每次下载前询问保存位置"
                disabled={busy || !normalizedDownloadDirectory}
                onClick={() =>
                  onDownloadSettingsChange(
                    normalizedDownloadDirectory,
                    !settings.askDownloadLocation,
                  )}
              >
                <span aria-hidden="true" />
              </button>
            </div>

            <div className="settings-row">
              <label htmlFor="settings-conflict">
                <strong>同名文件冲突</strong>
                <small>下载到的目录中已存在同名文件时的行为</small>
              </label>
              <select
                id="settings-conflict"
                value={settings.downloadConflictStrategy}
                disabled={busy || !onDownloadBehaviorChange}
                onChange={(event) =>
                  updateBehavior({
                    conflictStrategy: event.target.value as DownloadConflictStrategy,
                  })}
              >
                <option value="ask">每次询问</option>
                <option value="rename">自动加序号（推荐）</option>
                <option value="skip">跳过</option>
              </select>
            </div>

            <SwitchRow
              label="下载完成后打开所在文件夹"
              hint="覆盖单文件、文件夹、Vault 与恢复下载"
              checked={settings.openFolderAfterDownload}
              disabled={busy || !onDownloadBehaviorChange}
              onChange={(next) => updateBehavior({ openFolderAfterDownload: next })}
            />
            <SwitchRow
              label="下载完成后校验文件哈希"
              hint="服务端提供 hash 时会计算并对比"
              checked={settings.verifyHashAfterDownload}
              disabled={busy || !onDownloadBehaviorChange}
              onChange={(next) => updateBehavior({ verifyHashAfterDownload: next })}
            />

            <div className="settings-row">
              <label htmlFor="settings-keep-part">
                <strong>失败副本保留天数</strong>
                <small>过期的 .koofr-part-* 临时文件会在下次启动时清理</small>
              </label>
              <select
                id="settings-keep-part"
                value={settings.keepFailedPartDays}
                disabled={busy || !onDownloadBehaviorChange}
                onChange={(event) =>
                  updateBehavior({ keepFailedPartDays: Number(event.target.value) })}
              >
                <option value={0}>立即清理</option>
                <option value={1}>1 天</option>
                <option value={3}>3 天</option>
                <option value={7}>7 天</option>
                <option value={14}>14 天</option>
                <option value={30}>30 天</option>
              </select>
            </div>
          </section>
        ) : null}

        {activeSection === 'transfer' ? (
          <section className="settings-section">
            <header className="settings-section__heading">
              <RefreshCw aria-hidden="true" />
              <div>
                <h3>传输</h3>
                <p>并发、分卷、断点续传与带宽限制。</p>
              </div>
            </header>

            <div className="settings-row settings-row--compact-grid">
              <label htmlFor="settings-upload-concurrency">
                <strong>并发上传任务数</strong>
                <small>取值范围 1–16，建议 2</small>
              </label>
              <select
                id="settings-upload-concurrency"
                value={settings.uploadConcurrency}
                disabled={busy || !onTransferAdvancedChange}
                onChange={(event) =>
                  updateAdvanced({ uploadConcurrency: Number(event.target.value) })}
              >
                {[1, 2, 3, 4, 6, 8, 12, 16].map((value) => (
                  <option key={value} value={value}>
                    {value}
                  </option>
                ))}
              </select>
              <label htmlFor="settings-download-concurrency">
                <strong>并发下载任务数</strong>
                <small>取值范围 1–16，建议 3</small>
              </label>
              <select
                id="settings-download-concurrency"
                value={settings.downloadConcurrency}
                disabled={busy || !onTransferAdvancedChange}
                onChange={(event) =>
                  updateAdvanced({ downloadConcurrency: Number(event.target.value) })}
              >
                {[1, 2, 3, 4, 6, 8, 12, 16].map((value) => (
                  <option key={value} value={value}>
                    {value}
                  </option>
                ))}
              </select>
              <label htmlFor="settings-split-part">
                <strong>分卷上传默认 part 大小</strong>
                <small>用于分卷会话默认值，可在弹窗中调整</small>
              </label>
              <select
                id="settings-split-part"
                value={settings.defaultSplitPartMib}
                disabled={busy || !onTransferAdvancedChange}
                onChange={(event) =>
                  updateAdvanced({ defaultSplitPartMib: Number(event.target.value) })}
              >
                {[16, 32, 64, 128, 256, 512, 1024].map((value) => (
                  <option key={value} value={value}>
                    {value} MiB
                  </option>
                ))}
              </select>
              <label htmlFor="settings-auto-split">
                <strong>提示改用分卷的阈值</strong>
                <small>普通上传超过阈值时提示改用分卷</small>
              </label>
              <select
                id="settings-auto-split"
                value={settings.autoSplitThresholdMib}
                disabled={busy || !onTransferAdvancedChange}
                onChange={(event) =>
                  updateAdvanced({ autoSplitThresholdMib: Number(event.target.value) })}
              >
                {[128, 256, 512, 1024, 2048, 4096].map((value) => (
                  <option key={value} value={value}>
                    {value} MiB
                  </option>
                ))}
              </select>
            </div>

            <div className="settings-switch-row">
              <span>
                <strong>遇到网络错误时自动继续</strong>
                <small>只处理 network_error。下载和分卷上传从断点继续，普通上传会从头重试。</small>
              </span>
              <button
                className={`settings-switch${settings.autoRetryNetworkErrors ? ' settings-switch--on' : ''}`}
                type="button"
                role="switch"
                aria-checked={settings.autoRetryNetworkErrors}
                aria-label="遇到网络错误时自动继续"
                disabled={busy}
                onClick={() =>
                  onTransferSettingsChange({
                    autoRetryNetworkErrors: !settings.autoRetryNetworkErrors,
                    networkRetryLimit: settings.networkRetryLimit,
                    networkRetryIntervalSeconds: settings.networkRetryIntervalSeconds,
                  })}
              >
                <span aria-hidden="true" />
              </button>
            </div>
            <div className="settings-row settings-row--compact-grid">
              <label htmlFor="settings-network-retry-limit">
                <strong>最大重试次数</strong>
                <small>不包括最初的传输请求；可选择一直重试</small>
              </label>
              <select
                id="settings-network-retry-limit"
                aria-label="最大重试次数"
                value={settings.networkRetryLimit ?? 'unlimited'}
                disabled={busy || !settings.autoRetryNetworkErrors}
                onChange={(event) =>
                  onTransferSettingsChange({
                    autoRetryNetworkErrors: settings.autoRetryNetworkErrors,
                    networkRetryLimit:
                      event.target.value === 'unlimited' ? null : Number(event.target.value),
                    networkRetryIntervalSeconds: settings.networkRetryIntervalSeconds,
                  })}
              >
                <option value={3}>3 次</option>
                <option value={5}>5 次</option>
                <option value={8}>8 次</option>
                <option value={10}>10 次</option>
                <option value={20}>20 次</option>
                <option value={50}>50 次</option>
                <option value="unlimited">无限重试</option>
              </select>
              <label htmlFor="settings-network-retry-interval">
                <strong>重试间隔</strong>
                <small>每次网络错误后等待相同时间再继续</small>
              </label>
              <select
                id="settings-network-retry-interval"
                aria-label="重试间隔"
                value={settings.networkRetryIntervalSeconds}
                disabled={busy || !settings.autoRetryNetworkErrors}
                onChange={(event) =>
                  onTransferSettingsChange({
                    autoRetryNetworkErrors: settings.autoRetryNetworkErrors,
                    networkRetryLimit: settings.networkRetryLimit,
                    networkRetryIntervalSeconds: Number(event.target.value),
                  })}
              >
                <option value={1}>1 秒</option>
                <option value={3}>3 秒</option>
                <option value={5}>5 秒</option>
                <option value={10}>10 秒</option>
                <option value={15}>15 秒</option>
                <option value={30}>30 秒</option>
                <option value={60}>1 分钟</option>
              </select>
            </div>

            <header
              className="settings-section__heading settings-section__heading--nested"
            >
              <Gauge aria-hidden="true" />
              <div>
                <h3>带宽</h3>
                <p>
                  限速实时应用于所有文件传输；系统空闲满五分钟时可自动恢复全速。
                </p>
              </div>
            </header>
            <div className="settings-row settings-row--compact-grid">
              <label htmlFor="settings-bandwidth-mode">
                <strong>带宽模式</strong>
                <small>控制何时应用限速</small>
              </label>
              <select
                id="settings-bandwidth-mode"
                value={settings.bandwidthMode}
                disabled={busy || !onBandwidthChange}
                onChange={(event) =>
                  updateBandwidth({ mode: event.target.value as BandwidthMode })}
              >
                <option value="full">全速</option>
                <option value="limited">总是限速</option>
                <option value="idleFull">系统空闲时全速</option>
              </select>
              <label htmlFor="settings-bandwidth-global">
                <strong>全局上限（KB/s）</strong>
                <small>0 表示不限制</small>
              </label>
              <input
                id="settings-bandwidth-global"
                type="number"
                min={0}
                step={128}
                value={settings.globalBandwidthLimitKbps}
                disabled={busy || !onBandwidthChange}
                onChange={(event) =>
                  updateBandwidth({ globalKbps: Math.max(0, Number(event.target.value) || 0) })}
              />
              <label htmlFor="settings-bandwidth-per-task">
                <strong>单任务上限（KB/s）</strong>
                <small>0 表示不限制</small>
              </label>
              <input
                id="settings-bandwidth-per-task"
                type="number"
                min={0}
                step={128}
                value={settings.perTaskBandwidthLimitKbps}
                disabled={busy || !onBandwidthChange}
                onChange={(event) =>
                  updateBandwidth({ perTaskKbps: Math.max(0, Number(event.target.value) || 0) })}
              />
            </div>
          </section>
        ) : null}

        {activeSection === 'cache' ? (
          <section className="settings-section">
            <header className="settings-section__heading">
              <Database aria-hidden="true" />
              <div>
                <h3>文件信息缓存</h3>
                <p>减少重复网络请求；点击文件页面的刷新按钮仍会强制获取最新数据。</p>
              </div>
            </header>

            <div className="cache-mode-grid" role="radiogroup" aria-label="缓存位置">
              {CACHE_OPTIONS.map((option) => {
                const Icon = option.icon
                const selected = settings.cacheMode === option.id
                return (
                  <button
                    className={`cache-mode${selected ? ' cache-mode--selected' : ''}`}
                    type="button"
                    role="radio"
                    aria-checked={selected}
                    disabled={busy}
                    key={option.id}
                    onClick={() => onCacheModeChange(option.id)}
                  >
                    <Icon aria-hidden="true" />
                    <span>
                      <strong>{option.title}</strong>
                      <small>{option.description}</small>
                    </span>
                    <i aria-hidden="true" />
                  </button>
                )
              })}
            </div>
            {settings.cacheMode === 'disk' ? (
              <p className="settings-note">
                磁盘缓存包含普通文件名和远程路径，但不包含密码、令牌或文件内容。
              </p>
            ) : null}

            <div className="settings-row">
              <label htmlFor="cache-ttl">
                <strong>缓存有效期</strong>
                <small>超过有效期后会自动重新获取</small>
              </label>
              <select
                id="cache-ttl"
                value={settings.cacheTtlMinutes}
                disabled={busy || settings.cacheMode === 'off'}
                onChange={(event) => onCacheTtlChange(Number(event.target.value))}
              >
                <option value={5}>5 分钟</option>
                <option value={15}>15 分钟</option>
                <option value={60}>1 小时</option>
                <option value={360}>6 小时</option>
                <option value={1440}>24 小时</option>
              </select>
            </div>

            <div className="settings-storage">
              <span>当前缓存 {cacheDetails?.items ?? 0} 项</span>
              <span>磁盘占用 {formatBytes(cacheDetails?.bytes ?? 0)}</span>
              <button
                type="button"
                disabled={busy || settings.cachedItems === 0}
                onClick={onClearCache}
              >
                <Trash2 aria-hidden="true" />
                清除缓存
              </button>
            </div>

            <header
              className="settings-section__heading settings-section__heading--nested"
            >
              <HardDrive aria-hidden="true" />
              <div>
                <h3>应用工作目录</h3>
                <p>统一保存设置、传输检查点、下载历史、磁盘缓存和诊断日志。</p>
              </div>
            </header>
            <div className="settings-path-field">
              <label htmlFor="settings-work-directory">工作配置目录</label>
              <span className="path-field__control">
                <input
                  id="settings-work-directory"
                  value={workDirectory}
                  disabled={busy}
                  onChange={(event) => setWorkDirectory(event.target.value)}
                  onKeyDown={(event) => {
                    if (
                      event.key === 'Enter'
                      && normalizedWorkDirectory
                      && workDirectoryChanged
                    ) {
                      onWorkDirectoryChange(normalizedWorkDirectory, moveExisting)
                    }
                  }}
                />
                <button
                  type="button"
                  aria-label="选择应用工作目录"
                  title="选择文件夹"
                  disabled={busy || browsing !== null}
                  onClick={() => void browseWorkDirectory()}
                >
                  <FolderOpen aria-hidden="true" />
                </button>
              </span>
            </div>
            <label className="settings-migration-option">
              <input
                type="checkbox"
                checked={moveExisting}
                disabled={busy}
                onChange={(event) => setMoveExisting(event.target.checked)}
              />
              <span>
                <strong>移动当前工作目录中的全部文件</strong>
                <small>取消勾选会从空目录开始，原目录及其中的数据保持不变。</small>
              </span>
            </label>
            {settings.pendingWorkDirectory ? (
              <p
                className={`settings-note${settings.workDirectoryMigrationFailed ? ' settings-note--error' : ''}`}
              >
                {settings.workDirectoryMigrationFailed
                  ? '上次迁移未完成；应用仍保留可用数据，并会在下次启动时重试。'
                  : '目录更改已安排，将在下次启动、打开任何工作文件之前应用。'}
              </p>
            ) : null}
            <div className="settings-path-actions">
              <small>目标必须是空的本地文件夹，且不能位于当前工作目录内部或外层。</small>
              <button
                type="button"
                aria-label="保存工作目录"
                disabled={busy || !workDirectoryChanged || !normalizedWorkDirectory}
                onClick={() => onWorkDirectoryChange(normalizedWorkDirectory, moveExisting)}
              >
                保存并在重启后应用
              </button>
            </div>
          </section>
        ) : null}

        {activeSection === 'logs' ? (
          <section className="settings-section">
            <header className="settings-section__heading">
              <Bug aria-hidden="true" />
              <div>
                <h3>诊断日志</h3>
                <p>
                  用于定位上传、下载和网络错误；不会记录令牌、邮箱、文件名或完整路径。
                </p>
              </div>
            </header>

            <div className="settings-row settings-row--compact-grid">
              <label htmlFor="settings-log-level">
                <strong>记录级别</strong>
                <small>排查问题时可临时切换到“调试”</small>
              </label>
              <select
                id="settings-log-level"
                aria-label="记录级别"
                value={logLevel}
                disabled={busy}
                onChange={(event) => setLogLevel(event.target.value as LogLevel)}
              >
                <option value="error">仅错误</option>
                <option value="warn">警告及错误</option>
                <option value="info">常规信息</option>
                <option value="debug">调试信息</option>
              </select>
              <label htmlFor="settings-log-retention">
                <strong>保留时间</strong>
                <small>轮转日志超过此时间会自动删除</small>
              </label>
              <select
                id="settings-log-retention"
                aria-label="保留时间"
                value={logRetentionDays}
                disabled={busy}
                onChange={(event) => setLogRetentionDays(Number(event.target.value))}
              >
                <option value={3}>3 天</option>
                <option value={7}>7 天</option>
                <option value={14}>14 天</option>
                <option value={30}>30 天</option>
                <option value={90}>90 天</option>
              </select>
              <label htmlFor="settings-log-size">
                <strong>单文件上限</strong>
                <small>达到上限后创建新的日志文件</small>
              </label>
              <select
                id="settings-log-size"
                aria-label="单文件上限"
                value={logMaxFileSizeMb}
                disabled={busy}
                onChange={(event) => setLogMaxFileSizeMb(Number(event.target.value))}
              >
                <option value={1}>1 MB</option>
                <option value={5}>5 MB</option>
                <option value={10}>10 MB</option>
                <option value={25}>25 MB</option>
                <option value={50}>50 MB</option>
              </select>
            </div>
            <div className="settings-storage">
              <span>{settings.logFiles} 个日志文件</span>
              <span>磁盘占用 {formatBytes(settings.logDiskBytes)}</span>
              <button
                type="button"
                disabled={busy || !loggingChanged}
                onClick={() =>
                  onLoggingSettingsChange({
                    logLevel,
                    logRetentionDays,
                    logMaxFileSizeMb,
                  })}
              >
                保存日志设置
              </button>
              <button
                type="button"
                disabled={busy || settings.logFiles === 0}
                onClick={onClearLogs}
              >
                <Trash2 aria-hidden="true" />
                清除日志
              </button>
            </div>

            <header
              className="settings-section__heading settings-section__heading--nested"
            >
              <KeyRound aria-hidden="true" />
              <div>
                <h3>保存的登录信息</h3>
                <p>应用专用密码仅保存在 Windows 凭据管理器中，不会写入设置文件。</p>
              </div>
            </header>
            {settings.savedEmail ? (
              <div className="saved-login">
                <span className="saved-login__icon">
                  <ShieldCheck aria-hidden="true" />
                </span>
                <span>
                  <strong>{settings.savedEmail}</strong>
                  <small>下次启动时自动连接</small>
                </span>
                <button type="button" disabled={busy} onClick={onForgetLogin}>
                  删除
                </button>
              </div>
            ) : (
              <div className="saved-login saved-login--empty">
                登录时勾选“保存密码”即可启用自动连接。
              </div>
            )}
          </section>
        ) : null}

        {activeSection === 'links' ? (
          <section className="settings-section">
            <header className="settings-section__heading">
              <LinkIcon aria-hidden="true" />
              <div>
                <h3>分享链接默认值</h3>
                <p>创建新链接时填入的默认选项，随时可以在弹窗中修改。</p>
              </div>
            </header>

            <div className="settings-row">
              <label htmlFor="settings-link-kind">
                <strong>默认链接类型</strong>
                <small>下载链接分享文件；上传链接允许他人上传</small>
              </label>
              <select
                id="settings-link-kind"
                value={settings.linkDefaultKind}
                disabled={busy || !onLinkDefaultsChange}
                onChange={(event) =>
                  updateLinks({ kind: event.target.value as LinkDefaultKind })}
              >
                <option value="download">下载链接</option>
                <option value="upload">上传链接</option>
              </select>
            </div>
            <div className="settings-row">
              <label htmlFor="settings-link-expire">
                <strong>默认有效期</strong>
                <small>0 表示不过期</small>
              </label>
              <select
                id="settings-link-expire"
                value={settings.linkDefaultExpireDays}
                disabled={busy || !onLinkDefaultsChange}
                onChange={(event) =>
                  updateLinks({ expireDays: Number(event.target.value) })}
              >
                <option value={0}>永不过期</option>
                <option value={1}>1 天</option>
                <option value={7}>7 天</option>
                <option value={30}>30 天</option>
                <option value={90}>90 天</option>
                <option value={365}>365 天</option>
              </select>
            </div>
            <SwitchRow
              label="默认要求密码"
              hint="创建链接时预勾选“设置密码”"
              checked={settings.linkDefaultRequirePassword}
              disabled={busy || !onLinkDefaultsChange}
              onChange={(next) => updateLinks({ requirePassword: next })}
            />
            <p className="settings-note">
              说明：Koofr 服务端约束仍生效。默认值仅在创建界面中预填，不会绕过任何权限。
            </p>
          </section>
        ) : null}

        {activeSection === 'notifications' ? (
          <section className="settings-section">
            <header className="settings-section__heading">
              <Bell aria-hidden="true" />
              <div>
                <h3>通知</h3>
                <p>使用 Tauri 原生通知，仅包含任务显示名，不包含完整文件路径。</p>
              </div>
            </header>

            <SwitchRow
              label="传输完成时通知"
              hint="非活动窗口下会弹出系统通知"
              checked={settings.notifyOnComplete}
              disabled={busy || !onNotificationChange}
              onChange={(next) => updateNotifications({ notifyOnComplete: next })}
            />
            <SwitchRow
              label="通知声音"
              hint="使用系统默认提示音"
              checked={settings.notifySound}
              disabled={busy || !onNotificationChange}
              onChange={(next) => updateNotifications({ notifySound: next })}
            />
            <SwitchRow
              label="任务栏角标显示活动传输数"
              hint="仅当 Windows 支持 taskbar overlay 时生效"
              checked={settings.trayBadge}
              disabled={busy || !onNotificationChange}
              onChange={(next) => updateNotifications({ trayBadge: next })}
            />
            <SwitchRow
              label="显示传输悬浮窗"
              hint="置顶显示上传、下载或暂停状态；单击可打开传输面板"
              checked={settings.floatingWindowEnabled}
              disabled={busy || !onNotificationChange}
              onChange={(next) => updateNotifications({
                floatingWindowEnabled: next,
                floatingWindowPrompted: true,
              })}
            />
            {settings.floatingWindowEnabled ? (
              <>
                <SwitchRow
                  label="空闲或暂停时收为图标"
                  hint="任务完成提示结束后，悬浮窗收为单图标方块"
                  checked={settings.floatingWindowCompactEnabled}
                  disabled={busy || !onNotificationChange}
                  onChange={(next) => updateNotifications({ floatingWindowCompactEnabled: next })}
                />
                <SwitchRow
                  label="单击暂停或继续"
                  hint="对所有可操作传输执行暂停或继续"
                  checked={settings.floatingWindowClickToggleEnabled}
                  disabled={busy || !onNotificationChange}
                  onChange={(next) => updateNotifications({ floatingWindowClickToggleEnabled: next })}
                />
                <SwitchRow
                  label="双击打开应用"
                  hint="双击悬浮窗显示并聚焦 Koofr GUI"
                  checked={settings.floatingWindowDoubleClickOpenEnabled}
                  disabled={busy || !onNotificationChange}
                  onChange={(next) => updateNotifications({ floatingWindowDoubleClickOpenEnabled: next })}
                />
              </>
            ) : null}
          </section>
        ) : null}

        {activeSection === 'security' ? (
          <section className="settings-section">
            <header className="settings-section__heading">
              <ShieldCheck aria-hidden="true" />
              <div>
                <h3>隐私与安全</h3>
                <p>本机隐私偏好；账户凭据仍保留在 Windows 凭据管理器中。</p>
              </div>
            </header>

            <div className="settings-row">
              <label htmlFor="settings-idle-logout">
                <strong>空闲自动登出</strong>
                <small>无操作达到该时间后自动断开 Koofr 会话</small>
              </label>
              <select
                id="settings-idle-logout"
                value={settings.idleAutoLogoutMinutes}
                disabled={busy || !onSecurityChange}
                onChange={(event) =>
                  updateSecurity({ idleAutoLogoutMinutes: Number(event.target.value) })}
              >
                <option value={0}>不启用</option>
                <option value={5}>5 分钟</option>
                <option value={15}>15 分钟</option>
                <option value={30}>30 分钟</option>
                <option value={60}>1 小时</option>
                <option value={240}>4 小时</option>
              </select>
            </div>
          </section>
        ) : null}

        {activeSection === 'history' ? (
          <section className="settings-section">
            <header className="settings-section__heading">
              <History aria-hidden="true" />
              <div>
                <h3>下载历史</h3>
                <p>控制传输面板保留下载完成记录的时长与数量。</p>
              </div>
            </header>

            <div className="settings-row settings-row--compact-grid">
              <label htmlFor="settings-history-days">
                <strong>保留天数</strong>
                <small>0 表示永久保留</small>
              </label>
              <select
                id="settings-history-days"
                value={settings.downloadHistoryKeepDays}
                disabled={busy || !onHistoryPolicyChange}
                onChange={(event) =>
                  updateHistory({ keepDays: Number(event.target.value) })}
              >
                <option value={0}>永久</option>
                <option value={7}>7 天</option>
                <option value={30}>30 天</option>
                <option value={90}>90 天</option>
                <option value={365}>365 天</option>
              </select>
              <label htmlFor="settings-history-count">
                <strong>最多保留条数</strong>
                <small>0 表示不限</small>
              </label>
              <select
                id="settings-history-count"
                value={settings.downloadHistoryKeepCount}
                disabled={busy || !onHistoryPolicyChange}
                onChange={(event) =>
                  updateHistory({ keepCount: Number(event.target.value) })}
              >
                <option value={0}>不限</option>
                <option value={100}>100 条</option>
                <option value={500}>500 条</option>
                <option value={1000}>1000 条</option>
                <option value={5000}>5000 条</option>
              </select>
            </div>
            {onClearDownloadHistory ? (
              <div className="settings-storage">
                <button type="button" disabled={busy} onClick={onClearDownloadHistory}>
                  <Trash2 aria-hidden="true" />
                  清除已完成记录
                </button>
              </div>
            ) : null}
          </section>
        ) : null}

        {activeSection === 'about' ? (
          <section className="settings-section">
            <header className="settings-section__heading">
              <Info aria-hidden="true" />
              <div>
                <h3>关于 Koofr GUI</h3>
                <p>版本信息与项目链接。</p>
              </div>
            </header>
            {versionInfo ? (
              <div className="settings-about">
                <p>
                  <strong>版本</strong>
                  <span>{versionInfo.version}</span>
                </p>
                <p>
                  <strong>许可证</strong>
                  <span>{versionInfo.license}</span>
                </p>
                <p>
                  <strong>项目主页</strong>
                  <button type="button" onClick={() => onOpenExternalPage?.('homepage')}>
                    {versionInfo.homepageUrl}
                  </button>
                </p>
                <p>
                  <strong>发布页</strong>
                  <button type="button" onClick={() => onOpenExternalPage?.('releases')}>
                    {versionInfo.releasesUrl}
                  </button>
                </p>
                <p>
                  <strong>反馈问题</strong>
                  <button type="button" onClick={() => onOpenExternalPage?.('issues')}>
                    {versionInfo.issuesUrl}
                  </button>
                </p>
              </div>
            ) : (
              <p className="settings-note">版本信息暂不可用。</p>
            )}
            <p className="settings-note">
              Windows 安装包目前未做代码签名，若首次运行出现 SmartScreen 提示，请点击“更多信息”后继续运行。
            </p>
          </section>
        ) : null}

        {error ? (
          <div className="settings-error" role="alert">
            {error}
          </div>
        ) : null}
      </div>
    </div>
  )
})

function sectionLabel(id: SectionId): string {
  const section = SECTIONS.find((entry) => entry.id === id)
  return section ? section.label : ''
}

interface SwitchRowProps {
  readonly label: string
  readonly hint?: string
  readonly checked: boolean
  readonly disabled?: boolean
  readonly onChange: (next: boolean) => void
}

function SwitchRow({ label, hint, checked, disabled, onChange }: SwitchRowProps) {
  return (
    <div className="settings-switch-row">
      <span>
        <strong>{label}</strong>
        {hint ? <small>{hint}</small> : null}
      </span>
      <button
        className={`settings-switch${checked ? ' settings-switch--on' : ''}`}
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        disabled={disabled}
        onClick={() => onChange(!checked)}
      >
        <span aria-hidden="true" />
      </button>
    </div>
  )
}
