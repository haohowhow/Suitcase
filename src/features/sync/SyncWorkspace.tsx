import {
  ArrowLeft,
  ArrowLeftRight,
  Check,
  CheckCircle2,
  ChevronRight,
  Cloud,
  Folder,
  FolderOpen,
  FolderSync,
  HardDrive,
  LoaderCircle,
  Pause,
  Play,
  Plus,
  RefreshCw,
  Trash2,
  TriangleAlert,
} from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { Modal } from '../../components/Modal'
import { commandErrorMessage, koofr } from '../../services/koofr'
import type {
  KoofrMount,
  LocalFileSelection,
  NativeSyncCapability,
  RemoteFile,
  SyncIssue,
  SyncMode,
  SyncPairStatus,
  SyncPairSummary,
  SyncRootKind,
  SyncTaskSummary,
} from '../../types/backend'
import type { SyncManagerController, SyncSpeedSample } from '../../hooks/useSyncManager'
import { formatBytes, formatRate as formatTransferRate } from '../../utils/format'

interface SyncWorkspaceProps {
  readonly mounts: readonly KoofrMount[]
  readonly defaultMountId: string
  readonly defaultRemotePath: string
  readonly sync: SyncManagerController
}

type SyncFilter = 'all' | 'active' | 'attention' | 'paused'
type DetailTab = 'overview' | 'tasks' | 'issues'

const statusText: Record<SyncPairStatus, string> = {
  initializing: '准备中',
  scanning: '扫描中',
  syncing: '同步中',
  idle: '已同步',
  paused: '已暂停',
  offline: '等待网络',
  attention: '需要处理',
}

const taskOperationText: Record<SyncTaskSummary['operation'], string> = {
  mkdir_local: '创建本地文件夹',
  mkdir_remote: '创建云端文件夹',
  upload: '上传',
  download: '下载',
  delete_local: '删除本地项',
  delete_remote: '删除云端项',
  move_remote: '移动云端项',
  conflict: '创建冲突副本',
  type_conflict: '处理类型冲突',
}

const taskStateText: Record<SyncTaskSummary['state'], string> = {
  queued: '等待中',
  running: '进行中',
  waiting: '等待网络',
  completed: '已完成',
  failed: '失败',
}

function formatRate(bytes: number) {
  return formatTransferRate(bytes, '—')
}

function formatTime(value: number | null) {
  return value ? new Date(value).toLocaleString('zh-CN', { hour12: false }) : '尚未完成'
}

function folderBasename(path: string) {
  const normalized = path.replace(/[\\/]+$/, '')
  const segments = normalized.split(/[\\/]/).filter(Boolean)
  return segments.at(-1) || path
}

function isPairActive(pair: SyncPairSummary) {
  return pair.status === 'syncing' || pair.status === 'scanning' || pair.status === 'initializing'
}

function completionPercent(
  completedItems: number,
  totalItems: number,
  completedBytes: number,
  totalBytes: number,
) {
  if (totalItems > 0) return Math.min(100, Math.max(0, (completedItems / totalItems) * 100))
  if (totalBytes > 0) return Math.min(100, Math.max(0, (completedBytes / totalBytes) * 100))
  return null
}

function currentTaskSummary(pair: SyncPairSummary) {
  if (pair.status === 'initializing') return { title: '正在准备同步', detail: '正在恢复同步状态并连接两端。' }
  if (pair.status === 'scanning') return { title: '正在扫描两端差异', detail: '任务队列会在扫描完成后显示。' }
  if (pair.status === 'paused') return { title: '同步队列已暂停', detail: pair.pendingItems > 0 ? `仍有 ${pair.pendingItems} 项等待处理。` : '继续同步后会重新核对两端。' }
  if (pair.status === 'offline') return { title: '正在等待网络恢复', detail: pair.pendingItems > 0 ? `仍有 ${pair.pendingItems} 项等待处理。` : '联网并登录后会继续。' }
  if (pair.status === 'attention') return { title: '同步需要处理', detail: pair.issueCount > 0 ? `有 ${pair.issueCount} 个问题，请切换到“问题”查看。` : '请检查当前同步状态。' }
  if (pair.status === 'idle') return { title: '当前没有进行中的任务', detail: '本地与云端已完成本轮核对。' }

  const remaining = Math.max(pair.pendingItems, pair.totalItems - pair.completedItems, 0)
  if (remaining === 0) return { title: '正在完成同步收尾', detail: '文件已传输完毕，正在做最后确认。' }
  const uploadRemaining = Math.max(pair.upload.pendingItems, pair.upload.totalItems - pair.upload.completedItems, 0)
  const downloadRemaining = Math.max(pair.download.pendingItems, pair.download.totalItems - pair.download.completedItems, 0)
  const title = uploadRemaining > 0 && downloadRemaining === 0
    ? '正在处理上传队列'
    : downloadRemaining > 0 && uploadRemaining === 0
      ? '正在处理下载队列'
      : '正在处理同步队列'
  const current = pair.totalItems > 0 ? Math.min(pair.totalItems, pair.completedItems + 1) : 1
  return {
    title,
    detail: pair.totalItems > 0 ? `第 ${current}/${pair.totalItems} 项 · 剩余 ${remaining} 项` : `剩余 ${remaining} 项`,
  }
}

function parentPath(path: string) {
  return path === '/' ? '/' : path.split('/').slice(0, -1).join('/') || '/'
}

function SyncSpeedChart({ samples }: { readonly samples: readonly SyncSpeedSample[] }) {
  const points = samples.slice(-60)
  if (points.length < 2 || points.every((sample) => sample.uploadBytesPerSecond === 0 && sample.downloadBytesPerSecond === 0)) {
    return <div className="sync-speed-chart__empty">当前周期暂无速度数据</div>
  }
  const max = Math.max(1, ...points.flatMap((sample) => [sample.uploadBytesPerSecond, sample.downloadBytesPerSecond]))
  const line = (key: 'uploadBytesPerSecond' | 'downloadBytesPerSecond') => points.map((sample, index) => {
    const x = points.length === 1 ? 0 : (index / (points.length - 1)) * 100
    const y = 100 - (sample[key] / max) * 92
    return `${x},${y}`
  }).join(' ')
  return (
    <div className="sync-speed-chart" aria-label="最近 60 秒上下行速度曲线">
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" role="img">
        <polyline className="sync-speed-chart__download" points={line('downloadBytesPerSecond')} />
        <polyline className="sync-speed-chart__upload" points={line('uploadBytesPerSecond')} />
      </svg>
      <div className="sync-speed-chart__legend">
        <span><i className="sync-speed-chart__dot sync-speed-chart__dot--upload" />上传</span>
        <span><i className="sync-speed-chart__dot sync-speed-chart__dot--download" />下载</span>
      </div>
    </div>
  )
}

function EndpointStats({ label, stats }: { readonly label: string; readonly stats: SyncPairSummary['local'] }) {
  return (
    <div className="sync-detail-endpoint">
      <strong>{label}</strong>
      <span>{stats.fileCount} 个文件 · {stats.directoryCount} 个目录</span>
      <span>{formatBytes(stats.bytes)}</span>
    </div>
  )
}

function DirectionTaskProgress({ label, stats }: { readonly label: string; readonly stats: SyncPairSummary['upload'] }) {
  const remaining = Math.max(stats.pendingItems, stats.totalItems - stats.completedItems, 0)
  const percent = completionPercent(stats.completedItems, stats.totalItems, stats.completedBytes, stats.totalBytes) ?? 0
  return (
    <div className="sync-task-row">
      <div className="sync-task-row__heading">
        <strong>{label}队列</strong>
        <span>{stats.totalItems > 0 ? `${stats.completedItems}/${stats.totalItems} 项 · 剩余 ${remaining} 项` : '本周期无任务'}</span>
      </div>
      <div className="sync-task-row__bytes">
        {stats.totalBytes > 0 ? `${formatBytes(stats.completedBytes)}/${formatBytes(stats.totalBytes)}` : '无文件内容传输'}
      </div>
      <div
        className="sync-task-row__progress"
        role="progressbar"
        aria-label={`${label}队列进度`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(percent)}
      >
        <span style={{ width: `${percent}%` }} />
      </div>
    </div>
  )
}

export function SyncWorkspace({ mounts, defaultMountId, defaultRemotePath, sync }: SyncWorkspaceProps) {
  const { pairs, speedSamples } = sync
  const [issues, setIssues] = useState<Record<string, SyncIssue[]>>({})
  const [tasks, setTasks] = useState<Record<string, SyncTaskSummary[]>>({})
  const [filter, setFilter] = useState<SyncFilter>('all')
  const [refreshing, setRefreshing] = useState(false)

  // Guided Create Sync modal state
  const [createOpen, setCreateOpen] = useState(false)
  const [localSelection, setLocalSelection] = useState<LocalFileSelection | null>(null)
  const [mountId, setMountId] = useState(defaultMountId)
  const [remotePath, setRemotePath] = useState(defaultRemotePath || '/')
  const [syncMode, setSyncMode] = useState<SyncMode>('classic')
  const [rootKind, setRootKind] = useState<SyncRootKind>('custom')
  const [nativeCapability, setNativeCapability] = useState<NativeSyncCapability | null>(null)
  const [suggestedRoot, setSuggestedRoot] = useState('')
  const [remoteBrowserPath, setRemoteBrowserPath] = useState('/')
  const [remoteFolders, setRemoteFolders] = useState<RemoteFile[]>([])
  const [remoteBrowserOpen, setRemoteBrowserOpen] = useState(false)
  const [remoteBrowserBusy, setRemoteBrowserBusy] = useState(false)
  const [busy, setBusy] = useState(false)
  const [createError, setCreateError] = useState('')

  // Page-level feedback & modals
  const [notice, setNotice] = useState<{ kind: 'error' | 'info'; message: string } | null>(null)
  const [detailPairId, setDetailPairId] = useState<string | null>(null)
  const [detailTab, setDetailTab] = useState<DetailTab>('overview')
  const [pendingRemovePair, setPendingRemovePair] = useState<SyncPairSummary | null>(null)

  const primaryMounts = useMemo(
    () => mounts.filter((mount) => mount.isPrimary && mount.online && !mount.isShared && mount.mountType.toLowerCase() === 'device'),
    [mounts],
  )
  const detailPair = detailPairId ? pairs.find((pair) => pair.id === detailPairId) ?? null : null

  useEffect(() => {
    if (!primaryMounts.some((mount) => mount.id === mountId)) {
      setMountId(primaryMounts[0]?.id ?? '')
    }
  }, [mountId, primaryMounts])

  useEffect(() => {
    let disposed = false
    void Promise.all([koofr.nativeSyncCapability(), koofr.suggestedOnDemandSyncRoot()])
      .then(([capability, root]) => {
        if (!disposed) {
          setNativeCapability(capability)
          setSuggestedRoot(root)
        }
      })
      .catch(() => {
        if (!disposed) {
          setNativeCapability({
            supported: false,
            reason: '无法确认此电脑是否支持按需同步。',
            buildNumber: null,
            integrationNumber: null,
          })
        }
      })
    return () => { disposed = true }
  }, [])

  useEffect(() => {
    if (syncMode === 'onDemand' && rootKind === 'unified') {
      setRemotePath('/')
      setRemoteBrowserOpen(false)
    }
  }, [rootKind, syncMode])

  useEffect(() => {
    if (!detailPairId || detailTab === 'issues') return
    let disposed = false
    let loading = false
    const loadTasks = async () => {
      if (loading) return
      loading = true
      try {
        const next = await koofr.listSyncTasks(detailPairId)
        if (!disposed) setTasks((current) => ({ ...current, [detailPairId]: next }))
      } catch {
        // Task names are intentionally queried only while this detail view is open.
      } finally {
        loading = false
      }
    }
    void loadTasks()
    const timer = window.setInterval(() => void loadTasks(), 1000)
    return () => {
      disposed = true
      window.clearInterval(timer)
    }
  }, [detailPairId, detailTab])

  const openCreateModal = () => {
    setCreateError('')
    setRemoteBrowserOpen(false)
    setLocalSelection(null)
    setRemotePath(defaultRemotePath || '/')
    setSyncMode('classic')
    setRootKind('custom')
    setCreateOpen(true)
  }

  const closeCreateModal = () => {
    if (busy) return
    setCreateOpen(false)
    setCreateError('')
    setRemoteBrowserOpen(false)
  }

  const chooseFolder = async () => {
    try {
      const selected = await koofr.selectSyncFolder()
      if (selected) setLocalSelection(selected)
      setCreateError('')
    } catch (error) {
      setCreateError(commandErrorMessage(error, '无法选择本地文件夹'))
    }
  }

  const createPair = async () => {
    if (!localSelection || !mountId || !remotePath.trim()) return
    setBusy(true)
    setCreateError('')
    try {
      if (syncMode === 'onDemand') {
        await koofr.createOnDemandSyncPair(localSelection.grantId, mountId, remotePath.trim(), rootKind)
      } else {
        await koofr.createSyncPair(localSelection.grantId, mountId, remotePath.trim())
      }
      setLocalSelection(null)
      setRemoteBrowserOpen(false)
      setCreateOpen(false)
      setNotice(null)
      await sync.refresh()
    } catch (error) {
      setCreateError(commandErrorMessage(error, '无法创建同步。请确认 Koofr 文件夹可以写入，且没有与已有同步重叠'))
    } finally {
      setBusy(false)
    }
  }

  const browseRemote = async (path = remoteBrowserPath) => {
    if (!mountId) return
    setRemoteBrowserBusy(true)
    setCreateError('')
    try {
      const files = await koofr.listFiles(mountId, path)
      setRemoteFolders(files.filter((file) => file.entryType.toLowerCase() === 'dir'))
      setRemoteBrowserPath(path)
      setRemoteBrowserOpen(true)
    } catch (error) {
      setCreateError(commandErrorMessage(error, '无法读取 Koofr 文件夹'))
    } finally {
      setRemoteBrowserBusy(false)
    }
  }

  const chooseRemoteFolder = (path: string) => {
    setRemotePath(path)
    setRemoteBrowserPath(path)
    setRemoteBrowserOpen(false)
  }

  const refreshPairs = async () => {
    setRefreshing(true)
    try {
      await sync.refresh()
    } finally {
      setRefreshing(false)
    }
  }

  const showDetail = async (pair: SyncPairSummary, tab: DetailTab = 'overview') => {
    setDetailPairId(pair.id)
    setDetailTab(tab)
    if (tab === 'issues') {
      try {
        const next = await koofr.listSyncIssues(pair.id)
        setIssues((current) => ({ ...current, [pair.id]: next }))
      } catch (error) {
        setNotice({ kind: 'error', message: commandErrorMessage(error, '无法读取同步问题') })
      }
    }
  }

  const toggle = async (pair: SyncPairSummary) => {
    try {
      await sync.setPairPaused(pair.id, !pair.paused)
      await sync.refresh()
      setNotice(null)
    } catch (error) {
      setNotice({ kind: 'error', message: commandErrorMessage(error, '无法更新同步状态') })
    }
  }

  const confirmRemove = async () => {
    if (!pendingRemovePair) return
    const target = pendingRemovePair
    setPendingRemovePair(null)
    try {
      await sync.removePair(target.id)
      if (detailPairId === target.id) setDetailPairId(null)
      setNotice(null)
    } catch (error) {
      setNotice({ kind: 'error', message: commandErrorMessage(error, '无法移除同步') })
    }
  }

  const runNow = async (pair: SyncPairSummary) => {
    try {
      await sync.runPairNow(pair.id)
      setNotice(null)
    } catch (error) {
      setNotice({ kind: 'error', message: commandErrorMessage(error, '无法立即同步') })
    }
  }

  const migrate = async (pair: SyncPairSummary) => {
    try {
      const selection = await koofr.selectSyncFolder()
      if (!selection) return
      const kind: SyncRootKind = pair.remotePath === '/' ? 'unified' : 'custom'
      await koofr.migrateSyncPairToOnDemand(pair.id, selection.grantId, kind)
      await sync.refresh()
      setNotice({
        kind: 'info',
        message: '已切换为按需同步。原文件夹保留为完整副本，确认新位置无误后可自行删除。',
      })
    } catch (error) {
      setNotice({
        kind: 'error',
        message: commandErrorMessage(error, '切换未完成，原来的同步设置保持不变。'),
      })
    }
  }

  const openRoot = async (pair: SyncPairSummary) => {
    try {
      await koofr.openSyncRoot(pair.id)
    } catch (error) {
      setNotice({ kind: 'error', message: commandErrorMessage(error, '无法在资源管理器中打开同步文件夹') })
    }
  }

  // Summary metrics across all configured sync pairs
  const summary = useMemo(() => {
    let activeCount = 0
    let attentionCount = 0
    let pausedCount = 0
    let offlineCount = 0
    let classicCount = 0
    let onDemandCount = 0
    let totalUploadRate = 0
    let totalDownloadRate = 0
    let totalLocalFiles = 0
    let totalLocalBytes = 0

    for (const pair of pairs) {
      if (isPairActive(pair)) activeCount += 1
      if (pair.status === 'attention' || pair.issueCount > 0) attentionCount += 1
      if (pair.paused || pair.status === 'paused') pausedCount += 1
      if (pair.status === 'offline') offlineCount += 1
      if (pair.syncMode === 'onDemand') onDemandCount += 1
      else classicCount += 1
      totalUploadRate += pair.upload.bytesPerSecond
      totalDownloadRate += pair.download.bytesPerSecond
      totalLocalFiles += pair.local.fileCount
      totalLocalBytes += pair.local.bytes
    }

    return {
      activeCount,
      attentionCount,
      pausedCount,
      offlineCount,
      classicCount,
      onDemandCount,
      totalUploadRate,
      totalDownloadRate,
      totalLocalFiles,
      totalLocalBytes,
    }
  }, [pairs])

  const filteredPairs = useMemo(() => {
    if (filter === 'active') return pairs.filter(isPairActive)
    if (filter === 'attention') return pairs.filter((pair) => pair.status === 'attention' || pair.issueCount > 0)
    if (filter === 'paused') return pairs.filter((pair) => pair.paused || pair.status === 'paused')
    return pairs
  }, [filter, pairs])

  const effectiveFilter = filter !== 'all' && filteredPairs.length === 0 ? 'all' : filter
  const visiblePairs = effectiveFilter === 'all' ? pairs : filteredPairs

  const overallState = summary.attentionCount > 0
    ? { tone: 'attention', text: `${summary.attentionCount} 组需要处理` }
    : summary.activeCount > 0
      ? { tone: 'syncing', text: `${summary.activeCount} 组正在同步` }
      : summary.offlineCount > 0
        ? { tone: 'offline', text: `${summary.offlineCount} 组等待网络` }
        : summary.pausedCount === pairs.length && pairs.length > 0
          ? { tone: 'paused', text: '全部已暂停' }
          : { tone: 'idle', text: '全部已同步' }

  const detailSamples = detailPairId
    ? speedSamples.filter((sample) => sample.pairId === detailPairId).slice(-60)
    : []
  const detailTasks = detailPairId ? tasks[detailPairId] ?? [] : []
  const detailActiveTask = detailTasks.find((task) => task.state === 'running' || task.state === 'waiting')
    ?? detailTasks.find((task) => task.state === 'queued')
    ?? detailTasks.find((task) => task.state === 'failed')
  const detailProgress = detailPair
    ? completionPercent(detailPair.completedItems, detailPair.totalItems, detailPair.completedBytes, detailPair.totalBytes) ?? 0
    : 0
  const detailCurrentTask = detailPair ? currentTaskSummary(detailPair) : null

  const canCreate = !busy
    && Boolean(localSelection)
    && Boolean(mountId)
    && (syncMode === 'classic' || nativeCapability?.supported === true)
    && (syncMode === 'onDemand' && rootKind === 'unified' ? true : Boolean(remotePath.trim()))

  return (
    <main className="workspace sync-workspace" aria-label="同步">
      <div className="workspace__header sync-workspace__header">
        <div>
          <h1>同步文件夹</h1>
          <p className="workspace__description">
            让电脑上的文件夹与 Koofr 主存储保持一致，支持经典双向同步与 Windows 按需同步。
          </p>
        </div>
        <div className="top-actions">
          {pairs.length > 0 ? (
            <button
              className="icon-button icon-button--bordered"
              type="button"
              aria-label="刷新同步状态"
              title="刷新"
              disabled={refreshing}
              onClick={() => void refreshPairs()}
            >
              <RefreshCw className={refreshing ? 'icon-spin' : ''} size={15} strokeWidth={1.8} />
            </button>
          ) : null}
          <button className="primary-button" type="button" onClick={openCreateModal}>
            <Plus size={15} strokeWidth={2} />
            新建同步
          </button>
        </div>
      </div>

      <div className="sync-workspace__body">
        {notice ? (
          <div className={`notice ${notice.kind === 'error' ? 'notice--error' : 'notice--info'}`} role={notice.kind === 'error' ? 'alert' : 'status'}>
            {notice.message}
          </div>
        ) : null}

        {pairs.length > 0 ? (
          <section className="sync-overview" aria-label="同步状态总览">
            <div className="sync-overview__metrics">
              <div className="sync-metric">
                <span className="sync-metric__label">同步组</span>
                <strong className="sync-metric__value">{pairs.length} 组</strong>
                <small className="sync-metric__sub">
                  {summary.classicCount > 0 && summary.onDemandCount > 0
                    ? `经典 ${summary.classicCount} · 按需 ${summary.onDemandCount}`
                    : summary.onDemandCount > 0
                      ? '按需同步'
                      : '经典同步'}
                </small>
              </div>
              <div className="sync-metric">
                <span className="sync-metric__label">整体状态</span>
                <strong className="sync-metric__value sync-metric__status">
                  <span className={`sync-status__dot sync-status__dot--${overallState.tone}`} aria-hidden="true" />
                  {overallState.text}
                </strong>
                <small className="sync-metric__sub">
                  {summary.pausedCount > 0 && summary.pausedCount < pairs.length
                    ? `另有 ${summary.pausedCount} 组已暂停`
                    : '自动监听本地与云端变更'}
                </small>
              </div>
              <div className="sync-metric">
                <span className="sync-metric__label">
                  {summary.totalUploadRate > 0 || summary.totalDownloadRate > 0 ? '实时速率' : '已纳管本地文件'}
                </span>
                <strong className="sync-metric__value">
                  {summary.totalUploadRate > 0 || summary.totalDownloadRate > 0
                    ? `↑ ${formatRate(summary.totalUploadRate)} · ↓ ${formatRate(summary.totalDownloadRate)}`
                    : `${summary.totalLocalFiles} 个文件`}
                </strong>
                <small className="sync-metric__sub">
                  {summary.totalUploadRate > 0 || summary.totalDownloadRate > 0
                    ? `本地共 ${summary.totalLocalFiles} 个文件`
                    : formatBytes(summary.totalLocalBytes)}
                </small>
              </div>
            </div>

            {pairs.length > 1 ? (
              <div className="sync-filters" role="group" aria-label="筛选同步文件夹">
                <button
                  className={`sync-filter-chip${effectiveFilter === 'all' ? ' sync-filter-chip--active' : ''}`}
                  type="button"
                  onClick={() => setFilter('all')}
                >
                  全部 <span>{pairs.length}</span>
                </button>
                {summary.activeCount > 0 ? (
                  <button
                    className={`sync-filter-chip${effectiveFilter === 'active' ? ' sync-filter-chip--active' : ''}`}
                    type="button"
                    onClick={() => setFilter('active')}
                  >
                    进行中 <span>{summary.activeCount}</span>
                  </button>
                ) : null}
                {summary.attentionCount > 0 ? (
                  <button
                    className={`sync-filter-chip sync-filter-chip--warning${effectiveFilter === 'attention' ? ' sync-filter-chip--active' : ''}`}
                    type="button"
                    onClick={() => setFilter('attention')}
                  >
                    需处理 <span>{summary.attentionCount}</span>
                  </button>
                ) : null}
                {summary.pausedCount > 0 ? (
                  <button
                    className={`sync-filter-chip${effectiveFilter === 'paused' ? ' sync-filter-chip--active' : ''}`}
                    type="button"
                    onClick={() => setFilter('paused')}
                  >
                    已暂停 <span>{summary.pausedCount}</span>
                  </button>
                ) : null}
              </div>
            ) : null}
          </section>
        ) : null}

        {pairs.length === 0 ? (
          <div className="empty-state sync-empty">
            <span className="sync-empty__icon" aria-hidden="true">
              <FolderSync size={24} strokeWidth={1.7} />
            </span>
            <h2>还没有同步文件夹</h2>
            <p>将电脑上的文件夹与 Koofr 主存储关联，支持离线可用的经典同步，或节省磁盘空间的 Windows 按需同步。</p>
            <button className="primary-button" type="button" onClick={openCreateModal}>
              <Plus size={15} strokeWidth={2} />
              新建同步文件夹
            </button>
          </div>
        ) : (
          <div className="sync-list">
            {visiblePairs.map((pair) => {
              const active = isPairActive(pair)
              const percent = active
                ? completionPercent(pair.completedItems, pair.totalItems, pair.completedBytes, pair.totalBytes)
                : null
              const isOnDemand = pair.syncMode === 'onDemand'
              return (
                <article className={`sync-card sync-card--${pair.status}`} key={pair.id}>
                  <div className="sync-card__top">
                    <div className="sync-card__identity">
                      <span className={`sync-card__icon${isOnDemand ? ' sync-card__icon--ondemand' : ''}`} aria-hidden="true">
                        {isOnDemand ? <Cloud size={17} strokeWidth={1.8} /> : <FolderSync size={17} strokeWidth={1.8} />}
                      </span>
                      <div className="sync-card__title-group">
                        <div className="sync-card__heading">
                          <strong translate="no" title={pair.localRoot}>{folderBasename(pair.localRoot)}</strong>
                          <span className={`sync-mode-badge${isOnDemand ? ' sync-mode-badge--ondemand' : ''}`}>
                            {isOnDemand ? '按需同步' : '经典同步'}
                          </span>
                          <span className={`sync-status sync-status--${pair.status}`}>
                            <span className="sync-status__dot" />
                            {statusText[pair.status]}
                          </span>
                        </div>
                      </div>
                    </div>

                    <div className="sync-card__actions">
                      <button
                        className="icon-button"
                        type="button"
                        aria-label="在资源管理器中打开"
                        title="在资源管理器中打开"
                        onClick={() => void openRoot(pair)}
                      >
                        <FolderOpen size={15} strokeWidth={1.8} />
                      </button>
                      <button
                        className="icon-button"
                        type="button"
                        aria-label={pair.paused ? '继续同步' : '暂停同步'}
                        title={pair.paused ? '继续同步' : '暂停同步'}
                        onClick={() => void toggle(pair)}
                      >
                        {pair.paused ? <Play size={15} strokeWidth={1.8} /> : <Pause size={15} strokeWidth={1.8} />}
                      </button>
                      <button
                        className="icon-button"
                        type="button"
                        aria-label="立即同步"
                        title="立即同步"
                        onClick={() => void runNow(pair)}
                      >
                        <RefreshCw size={15} strokeWidth={1.8} />
                      </button>
                      <button
                        className="icon-button icon-button--danger"
                        type="button"
                        aria-label="移除同步"
                        title="移除同步"
                        onClick={() => setPendingRemovePair(pair)}
                      >
                        <Trash2 size={15} strokeWidth={1.8} />
                      </button>
                    </div>
                  </div>

                  <div className="sync-card__mapping">
                    <div className="sync-endpoint-pill">
                      <span className="sync-endpoint-pill__label">
                        <HardDrive size={12} strokeWidth={1.9} aria-hidden="true" />
                        本地文件夹
                      </span>
                      <strong translate="no" title={pair.localRoot}>{pair.localRoot}</strong>
                      <small>{pair.local.fileCount} 个文件 · {formatBytes(pair.local.bytes)}</small>
                    </div>
                    <span className="sync-card__mapping-arrow" aria-hidden="true">
                      <ArrowLeftRight size={14} strokeWidth={1.8} />
                    </span>
                    <div className="sync-endpoint-pill">
                      <span className="sync-endpoint-pill__label">
                        <Cloud size={12} strokeWidth={1.9} aria-hidden="true" />
                        Koofr 主存储
                      </span>
                      <strong translate="no" title={pair.remotePath}>{pair.remotePath}</strong>
                      <small>{pair.remote.fileCount} 个文件 · {formatBytes(pair.remote.bytes)}</small>
                    </div>
                  </div>

                  {isOnDemand ? (
                    <div className="sync-card__ondemand-strip">
                      <span className={`sync-ondemand-pill${pair.nativeConnected ? ' sync-ondemand-pill--connected' : ''}`}>
                        {pair.nativeConnected ? '已连接 Windows' : 'Windows 离线'}
                      </span>
                      <span>仅在线 {pair.onlineOnlyFiles} · 已下载 {pair.locallyAvailableFiles} · 始终保留 {pair.pinnedFiles}</span>
                    </div>
                  ) : null}

                  {active ? (
                    <div className="sync-card__live">
                      <div className="sync-card__meta">
                        <span>已完成 {pair.completedItems}/{pair.totalItems} 项 · {formatBytes(pair.completedBytes)}/{formatBytes(pair.totalBytes)}</span>
                        <span>↑ {formatRate(pair.upload.bytesPerSecond)}　↓ {formatRate(pair.download.bytesPerSecond)}</span>
                      </div>
                      {percent !== null ? (
                        <div
                          className="sync-card__progress"
                          role="progressbar"
                          aria-label={`${pair.localRoot} 同步进度`}
                          aria-valuemin={0}
                          aria-valuemax={100}
                          aria-valuenow={Math.round(percent)}
                        >
                          <span style={{ width: `${percent}%` }} />
                        </div>
                      ) : null}
                    </div>
                  ) : null}

                  {pair.issueCount > 0 ? (
                    <button
                      className="sync-card__issue-banner"
                      type="button"
                      onClick={() => void showDetail(pair, 'issues')}
                    >
                      <TriangleAlert size={14} aria-hidden="true" />
                      <span>{pair.issueCount} 个问题需要处理</span>
                      <em>查看问题</em>
                      <ChevronRight size={14} aria-hidden="true" />
                    </button>
                  ) : null}

                  <div className="sync-card__footer">
                    <span className="sync-card__timestamp">最近同步：{formatTime(pair.lastSyncedAt)}</span>
                    <div className="sync-card__footer-actions">
                      {!isOnDemand && nativeCapability?.supported ? (
                        <button
                          className="button button--subtle"
                          type="button"
                          onClick={() => void migrate(pair)}
                        >
                          切换为按需同步
                        </button>
                      ) : null}
                      <button
                        className="button button--subtle"
                        type="button"
                        onClick={() => void showDetail(pair, 'overview')}
                      >
                        详情
                      </button>
                    </div>
                  </div>
                </article>
              )
            })}
          </div>
        )}
      </div>

      {createOpen ? (
        <Modal
          title="新建同步文件夹"
          actionLabel={busy ? '正在创建…' : '确认创建'}
          actionDisabled={!canCreate}
          secondaryActionLabel="取消"
          secondaryActionDisabled={busy}
          wide
          onClose={closeCreateModal}
          onSecondaryAction={closeCreateModal}
          onAction={() => void createPair()}
        >
          <div className="sync-wizard">
            {createError ? <div className="notice notice--error" role="alert">{createError}</div> : null}

            <div className="sync-wizard__step">
              <span className="sync-wizard__label">1. 选择同步模式</span>
              <div className="sync-mode-grid" role="radiogroup" aria-label="同步模式">
                <button
                  type="button"
                  role="radio"
                  aria-checked={syncMode === 'classic'}
                  className={`sync-mode-card${syncMode === 'classic' ? ' sync-mode-card--selected' : ''}`}
                  disabled={busy}
                  onClick={() => setSyncMode('classic')}
                >
                  <span className="sync-mode-card__icon" aria-hidden="true">
                    <FolderSync size={17} strokeWidth={1.85} />
                  </span>
                  <span className="sync-mode-card__copy">
                    <strong>经典同步</strong>
                    <small>完整保存文件到本机，断网也能随时打开。</small>
                  </span>
                </button>
                <button
                  type="button"
                  role="radio"
                  aria-checked={syncMode === 'onDemand'}
                  className={`sync-mode-card${syncMode === 'onDemand' ? ' sync-mode-card--selected' : ''}`}
                  disabled={busy || nativeCapability?.supported === false}
                  onClick={() => setSyncMode('onDemand')}
                >
                  <span className="sync-mode-card__icon" aria-hidden="true">
                    <Cloud size={17} strokeWidth={1.85} />
                  </span>
                  <span className="sync-mode-card__copy">
                    <strong>按需同步（预览）</strong>
                    <small>在资源管理器中显示全部云端文件，打开时再下载。</small>
                  </span>
                </button>
              </div>
              {nativeCapability?.supported === false ? (
                <p className="sync-wizard__hint sync-wizard__hint--warn">
                  {nativeCapability.reason || '此 Windows 版本不支持按需同步。'}
                </p>
              ) : null}
            </div>

            <div className="sync-wizard__step">
              <span className="sync-wizard__label">2. 选择 Koofr 云端位置</span>

              {syncMode === 'onDemand' ? (
                <div className="sync-scope-toggle" role="radiogroup" aria-label="按需同步范围">
                  <button
                    type="button"
                    role="radio"
                    aria-checked={rootKind === 'custom'}
                    className={rootKind === 'custom' ? 'is-active' : ''}
                    disabled={busy}
                    onClick={() => setRootKind('custom')}
                  >
                    选择 Koofr 文件夹
                  </button>
                  <button
                    type="button"
                    role="radio"
                    aria-checked={rootKind === 'unified'}
                    className={rootKind === 'unified' ? 'is-active' : ''}
                    disabled={busy}
                    onClick={() => setRootKind('unified')}
                  >
                    同步整个 Koofr
                  </button>
                </div>
              ) : null}

              <div className="sync-wizard__row">
                <label className="sync-wizard__field sync-wizard__field--mount">
                  <span>Koofr 主存储</span>
                  <select
                    className="input"
                    value={mountId}
                    disabled={busy}
                    aria-label="Koofr 主存储"
                    onChange={(event) => setMountId(event.target.value)}
                  >
                    <option value="">选择 Koofr 主存储</option>
                    {primaryMounts.map((mount) => (
                      <option translate="no" key={mount.id} value={mount.id}>{mount.name}</option>
                    ))}
                  </select>
                </label>

                {syncMode === 'onDemand' && rootKind === 'unified' ? (
                  <div className="sync-wizard__field sync-wizard__field--path">
                    <span>同步范围</span>
                    <div className="sync-remote-picker--unified">
                      <div className="input sync-unified-target">
                        <Cloud size={15} strokeWidth={1.8} aria-hidden="true" />
                        <span>整个 Koofr</span>
                        <code>/</code>
                      </div>
                      <button
                        className="button"
                        type="button"
                        disabled={busy}
                        onClick={() => setRootKind('custom')}
                      >
                        改为选择文件夹
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="sync-wizard__field sync-wizard__field--path">
                    <label htmlFor="sync-remote-path">Koofr 文件夹</label>
                    <div className="sync-remote-picker__row">
                      <input
                        id="sync-remote-path"
                        className="input"
                        value={remotePath}
                        disabled={busy}
                        placeholder="例如 /My desktop sync"
                        aria-label="Koofr 文件夹"
                        onChange={(event) => setRemotePath(event.target.value)}
                      />
                      <button
                        className="button"
                        type="button"
                        disabled={busy || remoteBrowserBusy || !mountId}
                        onClick={() => {
                          if (remoteBrowserOpen) setRemoteBrowserOpen(false)
                          else void browseRemote(remotePath.trim() || '/')
                        }}
                      >
                        {remoteBrowserBusy ? <LoaderCircle className="spin" size={14} /> : '浏览 Koofr'}
                      </button>
                    </div>
                  </div>
                )}
              </div>

              {remoteBrowserOpen && !(syncMode === 'onDemand' && rootKind === 'unified') ? (
                <div className="sync-remote-browser" role="region" aria-label="选择 Koofr 文件夹">
                  <div className="sync-remote-picker__toolbar">
                    <button
                      className="sync-picker-action sync-picker-action--secondary"
                      type="button"
                      disabled={remoteBrowserBusy || remoteBrowserPath === '/'}
                      onClick={() => void browseRemote(parentPath(remoteBrowserPath))}
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
                      onClick={() => chooseRemoteFolder(remoteBrowserPath)}
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
                        onClick={() => void browseRemote(folder.path)}
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

            <div className="sync-wizard__step">
              <span className="sync-wizard__label">3. 选择本机文件夹</span>
              <div className="sync-local-picker">
                <div className={`sync-local-picker__path${localSelection ? '' : ' sync-local-picker__path--empty'}`}>
                  <Folder size={15} strokeWidth={1.8} aria-hidden="true" />
                  {localSelection?.localPath ? (
                    <span translate="no" title={localSelection.localPath}>{localSelection.localPath}</span>
                  ) : (
                    <span>尚未选择本机文件夹</span>
                  )}
                </div>
                <button
                  className="button"
                  type="button"
                  disabled={busy}
                  onClick={() => void chooseFolder()}
                >
                  选择本地文件夹
                </button>
              </div>
              {syncMode === 'onDemand' && rootKind === 'unified' && suggestedRoot ? (
                <p className="sync-wizard__hint">
                  “同步整个 Koofr”会同步主存储的全部内容，建议本地位置：<code translate="no">{suggestedRoot}</code>
                </p>
              ) : null}
            </div>

            <div className="sync-wizard__notes">
              <p>首次同步会合并两边已有内容；同名但内容不同时会保留冲突副本。</p>
              {syncMode === 'onDemand' ? (
                <p>按需同步会自动启用开机启动和关闭到托盘；完全退出应用时仅在线文件暂时无法打开。</p>
              ) : null}
            </div>
          </div>
        </Modal>
      ) : null}

      {pendingRemovePair ? (
        <Modal
          title="确认移除同步"
          actionLabel="确认移除"
          secondaryActionLabel="取消"
          onClose={() => setPendingRemovePair(null)}
          onAction={() => void confirmRemove()}
        >
          <p>
            {pendingRemovePair.syncMode === 'onDemand'
              ? <>解除“<span translate="no">{pendingRemovePair.localRoot}</span>”的按需同步？云端内容不受影响；已下载的文件会保留在本机，仅在线的文件会从本机移除。</>
              : <>移除“<span translate="no">{pendingRemovePair.localRoot}</span>”的同步？两边的文件都不会被删除。</>}
          </p>
        </Modal>
      ) : null}

      {detailPair ? (
        <Modal
          title={`同步详情 · ${folderBasename(detailPair.localRoot)}`}
          actionLabel="关闭"
          onClose={() => setDetailPairId(null)}
          extraWide
        >
          <div className="sync-detail-header">
            <div className="sync-detail-summary">
              <span className={`sync-status sync-status--${detailPair.status}`}>
                <span className="sync-status__dot" />
                {statusText[detailPair.status]}
              </span>
              <span className={`sync-mode-badge${detailPair.syncMode === 'onDemand' ? ' sync-mode-badge--ondemand' : ''}`}>
                {detailPair.syncMode === 'onDemand' ? '按需同步' : '经典同步'}
              </span>
              <span>最近扫描：{formatTime(detailPair.lastScanAt)}</span>
              <span>最近成功：{formatTime(detailPair.lastSyncedAt)}</span>
            </div>
            <div className="sync-detail-quick-actions">
              <button className="button button--subtle" type="button" onClick={() => void openRoot(detailPair)}>
                <FolderOpen size={14} strokeWidth={1.8} />
                打开目录
              </button>
              <button className="button button--subtle" type="button" onClick={() => void toggle(detailPair)}>
                {detailPair.paused ? <Play size={14} strokeWidth={1.8} /> : <Pause size={14} strokeWidth={1.8} />}
                {detailPair.paused ? '继续' : '暂停'}
              </button>
              <button className="button button--subtle" type="button" onClick={() => void runNow(detailPair)}>
                <RefreshCw size={14} strokeWidth={1.8} />
                立即同步
              </button>
            </div>
          </div>

          <div className="sync-detail-tabs" role="tablist" aria-label="同步详情分组">
            <button
              className={detailTab === 'overview' ? 'is-active' : ''}
              type="button"
              role="tab"
              aria-selected={detailTab === 'overview'}
              onClick={() => setDetailTab('overview')}
            >
              概览
            </button>
            <button
              className={detailTab === 'tasks' ? 'is-active' : ''}
              type="button"
              role="tab"
              aria-selected={detailTab === 'tasks'}
              onClick={() => setDetailTab('tasks')}
            >
              任务队列 {detailTasks.length > 0 ? `(${detailTasks.length})` : ''}
            </button>
            <button
              className={detailTab === 'issues' ? 'is-active' : ''}
              type="button"
              role="tab"
              aria-selected={detailTab === 'issues'}
              onClick={() => void showDetail(detailPair, 'issues')}
            >
              问题 {detailPair.issueCount > 0 ? `(${detailPair.issueCount})` : ''}
            </button>
          </div>

          {detailTab === 'overview' ? (
            <div className="sync-detail-pane">
              <div className="sync-detail-paths">
                <span translate="no">{detailPair.localRoot}</span>
                <span>↔</span>
                <span>Koofr 主存储 · <span translate="no">{detailPair.remotePath}</span></span>
              </div>

              <div className="sync-detail-endpoints">
                <EndpointStats label="本地" stats={detailPair.local} />
                <EndpointStats label="云端" stats={detailPair.remote} />
              </div>

              <div className="sync-detail-cycle">
                <h3>当前任务</h3>
                <div className="sync-current-task">
                  <strong>{detailCurrentTask?.title}</strong>
                  <span>{detailCurrentTask?.detail}</span>
                  {detailActiveTask ? (
                    <div className="sync-current-task__file">
                      <span>{taskOperationText[detailActiveTask.operation] ?? '同步'}</span>
                      <strong translate="no" title={detailActiveTask.displayName}>{detailActiveTask.displayName}</strong>
                      <small>{taskStateText[detailActiveTask.state]}</small>
                    </div>
                  ) : null}
                </div>
                <div className="sync-task-queue" aria-label="当前同步任务队列">
                  <DirectionTaskProgress label="上传" stats={detailPair.upload} />
                  <DirectionTaskProgress label="下载" stats={detailPair.download} />
                </div>
                <div
                  className="sync-card__progress"
                  role="progressbar"
                  aria-label="当前周期总进度"
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={Math.round(detailProgress)}
                >
                  <span style={{ width: `${detailProgress}%` }} />
                </div>
              </div>

              <div className="sync-detail-speeds">
                <div><strong>当前上传速度</strong><span>{formatRate(detailPair.upload.bytesPerSecond)}</span></div>
                <div><strong>当前下载速度</strong><span>{formatRate(detailPair.download.bytesPerSecond)}</span></div>
                <div>
                  <strong>最近 60 秒平均</strong>
                  <span>
                    {formatRate(detailSamples.length ? detailSamples.reduce((sum, sample) => sum + sample.uploadBytesPerSecond, 0) / detailSamples.length : 0)} ↑ ·{' '}
                    {formatRate(detailSamples.length ? detailSamples.reduce((sum, sample) => sum + sample.downloadBytesPerSecond, 0) / detailSamples.length : 0)} ↓
                  </span>
                </div>
                <div>
                  <strong>最近 60 秒峰值</strong>
                  <span>
                    {formatRate(Math.max(0, ...detailSamples.map((sample) => sample.uploadBytesPerSecond)))} ↑ ·{' '}
                    {formatRate(Math.max(0, ...detailSamples.map((sample) => sample.downloadBytesPerSecond)))} ↓
                  </span>
                </div>
              </div>
              <SyncSpeedChart samples={detailSamples} />
            </div>
          ) : detailTab === 'tasks' ? (
            <div className="sync-detail-pane">
              <div className="sync-current-task">
                <strong>{detailCurrentTask?.title}</strong>
                <span>{detailCurrentTask?.detail}</span>
              </div>
              <div className="sync-task-list">
                <div className="sync-task-list__header">
                  <strong>任务明细</strong>
                  <span>最近处理的文件</span>
                </div>
                {detailTasks.length > 0 ? detailTasks.map((task) => (
                  <div className={`sync-task-item sync-task-item--${task.state}`} key={task.id}>
                    <span className="sync-task-item__direction" aria-hidden="true">
                      {task.direction === 'upload' ? '↑' : task.direction === 'download' ? '↓' : '↕'}
                    </span>
                    <strong translate="no" title={task.displayName}>{task.displayName}</strong>
                    <span>{taskOperationText[task.operation] ?? '同步'}</span>
                    <small>{task.bytes > 0 ? formatBytes(task.bytes) : '—'} · {taskStateText[task.state]}</small>
                  </div>
                )) : (
                  <p className="sync-task-list__empty">
                    {detailPair.status === 'scanning' || detailPair.status === 'initializing'
                      ? '正在扫描，任务明细将在队列生成后显示。'
                      : '当前没有可显示的同步任务。'}
                  </p>
                )}
              </div>
            </div>
          ) : (
            <div className="sync-detail-pane">
              {issues[detailPair.id]?.length ? (
                <div className="sync-issues sync-issues--detail">
                  {issues[detailPair.id].map((issue) => (
                    <div className="sync-issue" key={issue.id}>
                      <TriangleAlert size={14} aria-hidden="true" />
                      <span>
                        {issue.relativePath ? <><span translate="no">{issue.relativePath}</span>: </> : '同步位置：'}
                        {issue.message}
                      </span>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="sync-issues-empty">
                  <CheckCircle2 size={18} strokeWidth={1.8} aria-hidden="true" />
                  <span>当前没有未解决问题。</span>
                </div>
              )}
            </div>
          )}
        </Modal>
      ) : null}
    </main>
  )
}
