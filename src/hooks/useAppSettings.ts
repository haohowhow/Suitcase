import { useCallback, useState } from 'react'
import {
  commandErrorMessage,
  ensureNotificationPermission,
  koofr,
} from '../services/koofr'
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
} from '../types/backend'

export function useAppSettings(
  showNotice: (message: string) => void,
  onEmailCleared?: () => void,
) {
  const [settings, setSettings] = useState<AppSettings | null>(null)
  const [settingsLoading, setSettingsLoading] = useState(false)
  const [settingsBusy, setSettingsBusy] = useState(false)
  const [settingsError, setSettingsError] = useState('')
  const [downloadSettingsError, setDownloadSettingsError] = useState('')
  const [versionInfo, setVersionInfo] = useState<AppVersionInfo | null>(null)

  const openSettings = useCallback(async () => {
    setSettingsLoading(true)
    setSettingsError('')
    setDownloadSettingsError('')
    try {
      const [next, version] = await Promise.all([
        koofr.getSettings(),
        koofr.appVersionInfo().catch(() => null),
      ])
      setSettings(next)
      if (version) setVersionInfo(version)
    } catch (error) {
      setSettingsError(commandErrorMessage(error, '无法读取本地设置。'))
    } finally {
      setSettingsLoading(false)
    }
  }, [])

  const runSettingsMutation = useCallback(
    async (
      mutate: () => Promise<AppSettings>,
      successMessage: string,
      failureMessage: string,
    ) => {
      setSettingsBusy(true)
      setSettingsError('')
      try {
        setSettings(await mutate())
        showNotice(successMessage)
      } catch (error) {
        setSettingsError(commandErrorMessage(error, failureMessage))
      } finally {
        setSettingsBusy(false)
      }
    },
    [showNotice],
  )

  const updateCacheSettings = useCallback(
    async (cacheMode: CacheMode, cacheTtlMinutes: number) => {
      setSettingsBusy(true)
      setSettingsError('')
      try {
        setSettings(await koofr.updateSettings(cacheMode, cacheTtlMinutes))
      } catch (error) {
        setSettingsError(commandErrorMessage(error, '无法保存缓存设置。'))
      } finally {
        setSettingsBusy(false)
      }
    },
    [],
  )

  const updateFileScanConcurrency = useCallback(
    (concurrency: number) => {
      void runSettingsMutation(
        () => koofr.updateFileScanConcurrency(concurrency),
        '扫描并发设置已保存',
        '无法保存扫描并发设置。',
      )
    },
    [runSettingsMutation],
  )

  const updateLoggingSettings = useCallback(
    async (
      logLevel: LogLevel,
      logRetentionDays: number,
      logMaxFileSizeMb: number,
    ) => {
      setSettingsBusy(true)
      setSettingsError('')
      try {
        setSettings(
          await koofr.updateLoggingSettings(
            logLevel,
            logRetentionDays,
            logMaxFileSizeMb,
          ),
        )
        showNotice('日志设置已保存')
      } catch (error) {
        setSettingsError(
          commandErrorMessage(error, '无法保存日志设置。'),
        )
      } finally {
        setSettingsBusy(false)
      }
    },
    [showNotice],
  )

  const updateTransferSettings = useCallback(
    async (
      autoRetryNetworkErrors: boolean,
      networkRetryLimit: number | null,
      networkRetryIntervalSeconds: number,
    ) => {
      setSettingsBusy(true)
      setSettingsError('')
      try {
        setSettings(
          await koofr.updateTransferSettings(
            autoRetryNetworkErrors,
            networkRetryLimit,
            networkRetryIntervalSeconds,
          ),
        )
        showNotice('传输设置已保存')
      } catch (error) {
        setSettingsError(commandErrorMessage(error, '无法保存传输设置。'))
      } finally {
        setSettingsBusy(false)
      }
    },
    [showNotice],
  )

  const browseWorkDirectory = useCallback(async () => {
    setSettingsError('')
    try {
      return await koofr.selectWorkDirectory()
    } catch (error) {
      setSettingsError(commandErrorMessage(error, '无法打开文件夹选择器。'))
      return null
    }
  }, [])

  const updateWorkDirectory = useCallback(
    async (workDirectory: string, moveExisting: boolean) => {
      setSettingsBusy(true)
      setSettingsError('')
      try {
        setSettings(
          await koofr.updateWorkDirectory(workDirectory, moveExisting),
        )
        showNotice('数据位置已保存，将在下次启动时生效')
      } catch (error) {
        setSettingsError(
          commandErrorMessage(
            error,
            '无法更改数据位置。请选择一个空的本地文件夹。',
          ),
        )
      } finally {
        setSettingsBusy(false)
      }
    },
    [showNotice],
  )

  const updateDownloadSettings = useCallback(
    async (
      nextDownloadDirectory: string,
      askDownloadLocation: boolean,
    ) => {
      setSettingsBusy(true)
      setSettingsError('')
      setDownloadSettingsError('')
      try {
        const nextSettings = await koofr.updateDownloadSettings(
          nextDownloadDirectory,
          askDownloadLocation,
        )
        setSettings(nextSettings)
        showNotice('下载设置已保存')
      } catch (error) {
        setDownloadSettingsError(
          commandErrorMessage(error, '无法保存下载设置，请检查文件夹路径。'),
        )
      } finally {
        setSettingsBusy(false)
      }
    },
    [showNotice],
  )

  const browseSettingsDownloadDirectory = useCallback(async () => {
    setDownloadSettingsError('')
    try {
      return await koofr.selectDownloadDirectory()
    } catch (error) {
      setDownloadSettingsError(
        commandErrorMessage(error, '无法打开下载文件夹选择器。'),
      )
      return null
    }
  }, [])

  const clearMetadataCache = useCallback(async () => {
    setSettingsBusy(true)
    setSettingsError('')
    try {
      setSettings(await koofr.clearMetadataCache())
      showNotice('文件信息缓存已清除')
    } catch (error) {
      setSettingsError(commandErrorMessage(error, '无法清除文件信息缓存。'))
    } finally {
      setSettingsBusy(false)
    }
  }, [showNotice])

  const clearLogs = useCallback(async () => {
    setSettingsBusy(true)
    setSettingsError('')
    try {
      setSettings(await koofr.clearLogs())
      showNotice('诊断日志已清除')
    } catch (error) {
      setSettingsError(commandErrorMessage(error, '无法清除诊断日志。'))
    } finally {
      setSettingsBusy(false)
    }
  }, [showNotice])

  const forgetSavedLogin = useCallback(async () => {
    setSettingsBusy(true)
    setSettingsError('')
    try {
      const next = await koofr.forgetSavedLogin()
      setSettings(next)
      onEmailCleared?.()
      showNotice('已从 Windows 凭据管理器删除登录信息')
    } catch (error) {
      setSettingsError(commandErrorMessage(error, '无法删除保存的登录信息。'))
    } finally {
      setSettingsBusy(false)
    }
  }, [onEmailCleared, showNotice])

  const updateAppearance = useCallback(
    (next: {
      language: InterfaceLanguage
      reduceMotion: boolean
      density: UiDensity
    }) =>
      void runSettingsMutation(
        () =>
          koofr.updateAppearanceSettings(
            next.language,
            next.reduceMotion,
            next.density,
          ),
        '外观设置已保存',
        '无法保存外观设置。',
      ),
    [runSettingsMutation],
  )

  const updateGeneral = useCallback(
    (next: {
      launchAtLogin: boolean
      closeToTray: boolean
      minimizeToTray: boolean
      confirmQuitWithTransfers: boolean
    }) =>
      void runSettingsMutation(
        () =>
          koofr.updateGeneralSettings(
            next.launchAtLogin,
            next.closeToTray,
            next.minimizeToTray,
            next.confirmQuitWithTransfers,
          ),
        '通用设置已保存',
        '无法保存通用设置。',
      ),
    [runSettingsMutation],
  )

  const updateAdvancedTransfer = useCallback(
    (next: {
      uploadConcurrency: number
      downloadConcurrency: number
      defaultSplitPartMib: number
      autoSplitThresholdMib: number
    }) =>
      void runSettingsMutation(
        () =>
          koofr.updateTransferAdvancedSettings(
            next.uploadConcurrency,
            next.downloadConcurrency,
            next.defaultSplitPartMib,
            next.autoSplitThresholdMib,
          ),
        '传输高级设置已保存',
        '无法保存传输高级设置。',
      ),
    [runSettingsMutation],
  )

  const updateDownloadBehavior = useCallback(
    (next: {
      conflictStrategy: DownloadConflictStrategy
      openFolderAfterDownload: boolean
      verifyHashAfterDownload: boolean
      keepFailedPartDays: number
    }) =>
      void runSettingsMutation(
        () =>
          koofr.updateDownloadBehaviorSettings(
            next.conflictStrategy,
            next.openFolderAfterDownload,
            next.verifyHashAfterDownload,
            next.keepFailedPartDays,
          ),
        '下载行为已保存',
        '无法保存下载行为。',
      ),
    [runSettingsMutation],
  )

  const updateBandwidth = useCallback(
    (next: { globalKbps: number; perTaskKbps: number; mode: BandwidthMode }) =>
      void runSettingsMutation(
        () =>
          koofr.updateBandwidthSettings(
            next.globalKbps,
            next.perTaskKbps,
            next.mode,
          ),
        '带宽设置已保存',
        '无法保存带宽设置。',
      ),
    [runSettingsMutation],
  )

  const updateNotifications = useCallback(
    (next: {
      notifyOnComplete: boolean
      notifySound: boolean
      trayBadge: boolean
      floatingWindowEnabled: boolean
      floatingWindowPrompted: boolean
      floatingWindowCompactEnabled: boolean
      floatingWindowClickToggleEnabled: boolean
      floatingWindowDoubleClickOpenEnabled: boolean
    }) => {
      void (async () => {
        if (
          next.notifyOnComplete &&
          !settings?.notifyOnComplete &&
          !(await ensureNotificationPermission())
        ) {
          setSettingsError('Windows 未获得系统通知权限，传输完成通知没有开启。')
          return
        }
        const floatingOnly = Boolean(
          settings &&
            next.notifyOnComplete === settings.notifyOnComplete &&
            next.notifySound === settings.notifySound &&
            next.trayBadge === settings.trayBadge,
        )
        await runSettingsMutation(
          () =>
            koofr.updateNotificationSettings(
              next.notifyOnComplete,
              next.notifySound,
              next.trayBadge,
              next.floatingWindowEnabled,
              next.floatingWindowPrompted,
              next.floatingWindowCompactEnabled,
              next.floatingWindowClickToggleEnabled,
              next.floatingWindowDoubleClickOpenEnabled,
            ),
          floatingOnly ? '悬浮窗设置已保存' : '通知设置已保存',
          floatingOnly ? '无法保存悬浮窗设置。' : '无法保存通知设置。',
        )
      })()
    },
    [runSettingsMutation, settings],
  )

  /** Saves the first-run floating window answer. Resolves to whether it was saved. */
  const answerFloatingWindowPrompt = useCallback(
    async (enabled: boolean): Promise<boolean> => {
      if (!settings) return false
      setSettingsBusy(true)
      setSettingsError('')
      try {
        const next = await koofr.updateNotificationSettings(
          settings.notifyOnComplete,
          settings.notifySound,
          settings.trayBadge,
          enabled,
          true,
          settings.floatingWindowCompactEnabled,
          settings.floatingWindowClickToggleEnabled,
          settings.floatingWindowDoubleClickOpenEnabled,
        )
        setSettings(next)
        showNotice(enabled ? '传输悬浮窗已开启' : '可稍后在“通知与悬浮窗”设置中开启悬浮窗')
        return true
      } catch (error) {
        const message = commandErrorMessage(error, '无法保存悬浮窗设置。')
        setSettingsError(message)
        // The prompt is not inside the settings panel, so the error has to be
        // visible on its own.
        showNotice(message)
        return false
      } finally {
        setSettingsBusy(false)
      }
    },
    [settings, showNotice],
  )

  const updateLinkDefaults = useCallback(
    (next: {
      kind: LinkDefaultKind
      expireDays: number
      requirePassword: boolean
    }) =>
      void runSettingsMutation(
        () =>
          koofr.updateLinkDefaults(
            next.kind,
            next.expireDays,
            next.requirePassword,
          ),
        '链接默认设置已保存',
        '无法保存链接默认设置。',
      ),
    [runSettingsMutation],
  )

  const updateSecurity = useCallback(
    (next: { idleAutoLogoutMinutes: number }) =>
      void runSettingsMutation(
        () => koofr.updateSecuritySettings(next.idleAutoLogoutMinutes),
        '安全设置已保存',
        '无法保存安全设置。',
      ),
    [runSettingsMutation],
  )

  const updateHistoryPolicy = useCallback(
    (next: { keepDays: number; keepCount: number }) =>
      void runSettingsMutation(
        () => koofr.updateHistoryPolicy(next.keepDays, next.keepCount),
        '下载历史设置已保存',
        '无法保存下载历史设置。',
      ),
    [runSettingsMutation],
  )

  return {
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
    updateFileScanConcurrency,
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
    runSettingsMutation,
  }
}
