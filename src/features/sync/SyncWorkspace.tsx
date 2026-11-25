import {
  ArrowLeft,
  Check,
  CheckCircle2,
  ChevronRight,
  Cloud,
  Folder,
  FolderOpen,
  FolderSync,
  LoaderCircle,
  Pause,
  Play,
  RefreshCw,
  Trash2,
  TriangleAlert,
} from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { Modal } from '../../components/Modal'
import { commandErrorMessage, koofr } from '../../services/koofr'
import type {
  KoofrMount,
  LocalFileSelection,
  RemoteFile,
  SyncIssue,
  SyncMode,
  SyncPairStatus,
  SyncPairSummary,
  SyncRootKind,
  SyncTaskSummary,
  NativeSyncCapability,
} from '../../types/backend'
import type { SyncManagerController, SyncSpeedSample } from '../../hooks/useSyncManager'
import { formatBytes, formatRate as formatTransferRate } from '../../utils/format'

interface SyncWorkspaceProps {
  readonly mounts: readonly KoofrMount[]
  readonly defaultMountId: string
  readonly defaultRemotePath: string
  readonly sync: SyncManagerController
}

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

function completionPercent(completedItems: number, totalItems: number, completedBytes: number, totalBytes: number) {
  if (totalItems > 0) return Math.min(100, Math.max(0, completedItems / totalItems * 100))
  if (totalBytes > 0) return Math.min(100, Math.max(0, completedBytes / totalBytes * 100))
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
  return { title, detail: pair.totalItems > 0 ? `第 ${current}/${pair.totalItems} 项 · 剩余 ${remaining} 项` : `剩余 ${remaining} 项` }
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
      <div className="sync-speed-chart__legend"><span><i className="sync-speed-chart__dot sync-speed-chart__dot--upload" />上传</span><span><i className="sync-speed-chart__dot sync-speed-chart__dot--download" />下载</span></div>
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
      <div className="sync-task-row__heading"><strong>{label}队列</strong><span>{stats.totalItems > 0 ? `${stats.completedItems}/${stats.totalItems} 项 · 剩余 ${remaining} 项` : '本周期无任务'}</span></div>
      <div className="sync-task-row__bytes">{stats.totalBytes > 0 ? `${formatBytes(stats.completedBytes)}/${formatBytes(stats.totalBytes)}` : '无文件内容传输'}</div>
      <div className="sync-task-row__progress" role="progressbar" aria-label={`${label}队列进度`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)}><span style={{ width: `${percent}%` }} /></div>
    </div>
  )
}

export function SyncWorkspace({ mounts, defaultMountId, defaultRemotePath, sync }: SyncWorkspaceProps) {
  const { pairs, speedSamples } = sync
  const [issues, setIssues] = useState<Record<string, SyncIssue[]>>({})
  const [tasks, setTasks] = useState<Record<string, SyncTaskSummary[]>>({})
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
  const [notice, setNotice] = useState('')
  const [detailPairId, setDetailPairId] = useState<string | null>(null)
  const [detailSection, setDetailSection] = useState<'overview' | 'issues'>('overview')
  const [pendingRemovePair, setPendingRemovePair] = useState<SyncPairSummary | null>(null)
  const remotePickerRef = useRef<HTMLDivElement | null>(null)
  const detailIssuesRef = useRef<HTMLDivElement | null>(null)

  const primaryMounts = useMemo(
    () => mounts.filter((mount) => mount.isPrimary && mount.online && !mount.isShared && mount.mountType.toLowerCase() === 'device'),
    [mounts],
  )
  const detailPair = detailPairId ? pairs.find((pair) => pair.id === detailPairId) ?? null : null

  useEffect(() => {
    if (!primaryMounts.some((mount) => mount.id === mountId)) setMountId(primaryMounts[0]?.id ?? '')
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
        if (!disposed) setNativeCapability({ supported: false, reason: '无法确认此电脑是否支持按需同步。', buildNumber: null, integrationNumber: null })
      })
    return () => { disposed = true }
  }, [])

  useEffect(() => {
    if (syncMode === 'onDemand' && rootKind === 'unified') setRemotePath('/')
  }, [rootKind, syncMode])

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (remotePickerRef.current && !remotePickerRef.current.contains(event.target as Node)) setRemoteBrowserOpen(false)
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setRemoteBrowserOpen(false)
    }
    document.addEventListener('pointerdown', onPointerDown)
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      document.removeEventListener('keydown', onKeyDown)
    }
  }, [])

  useEffect(() => {
    if (detailSection === 'issues') detailIssuesRef.current?.scrollIntoView?.({ block: 'start' })
  }, [detailSection])

  useEffect(() => {
    if (!detailPairId || detailSection !== 'overview') return
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
        // A summary event can still keep the aggregate progress current if polling fails.
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
  }, [detailPairId, detailSection])

  const chooseFolder = async () => {
    try {
      setLocalSelection(await koofr.selectSyncFolder())
      setNotice('')
    } catch (error) {
      setNotice(commandErrorMessage(error, '无法选择本地文件夹'))
    }
  }

  const createPair = async () => {
    if (!localSelection || !mountId || !remotePath.trim()) return
    setBusy(true)
    try {
      if (syncMode === 'onDemand') {
        await koofr.createOnDemandSyncPair(localSelection.grantId, mountId, remotePath.trim(), rootKind)
      } else {
        await koofr.createSyncPair(localSelection.grantId, mountId, remotePath.trim())
      }
      setLocalSelection(null)
      await sync.refresh()
    } catch (error) {
      setNotice(commandErrorMessage(error, '无法创建同步。请确认 Koofr 文件夹可以写入，且没有与已有同步重叠'))
    } finally {
      setBusy(false)
    }
  }

  const browseRemote = async (path = remoteBrowserPath) => {
    if (!mountId) return
    setRemoteBrowserBusy(true)
    try {
      const files = await koofr.listFiles(mountId, path)
      setRemoteFolders(files.filter((file) => file.entryType.toLowerCase() === 'dir'))
      setRemoteBrowserPath(path)
      setRemoteBrowserOpen(true)
      setNotice('')
    } catch (error) {
      setNotice(commandErrorMessage(error, '无法读取 Koofr 文件夹'))
    } finally {
      setRemoteBrowserBusy(false)
    }
  }

  const chooseRemoteFolder = (path: string) => {
    setRemotePath(path)
    setRemoteBrowserPath(path)
    setRemoteBrowserOpen(false)
  }

  const showDetail = async (pair: SyncPairSummary, section: 'overview' | 'issues' = 'overview') => {
    setDetailPairId(pair.id)
    setDetailSection(section)
    if (section === 'issues' && !issues[pair.id]) {
      try {
        const next = await koofr.listSyncIssues(pair.id)
        setIssues((current) => ({ ...current, [pair.id]: next }))
      } catch (error) {
        setNotice(commandErrorMessage(error, '无法读取同步问题'))
      }
    }
  }

  const toggle = async (pair: SyncPairSummary) => {
    try {
      await sync.setPairPaused(pair.id, !pair.paused)
      await sync.refresh()
    } catch (error) {
      setNotice(commandErrorMessage(error, '无法更新同步状态'))
    }
  }

  const confirmRemove = async () => {
    if (!pendingRemovePair) return
    const target = pendingRemovePair
    setPendingRemovePair(null)
    try {
      await sync.removePair(target.id)
    } catch (error) {
      setNotice(commandErrorMessage(error, '无法移除同步'))
    }
  }

  const runNow = async (pair: SyncPairSummary) => {
    try {
      await sync.runPairNow(pair.id)
    } catch (error) {
      setNotice(commandErrorMessage(error, '无法立即同步'))
    }
  }

  const migrate = async (pair: SyncPairSummary) => {
    try {
      const selection = await koofr.selectSyncFolder()
      if (!selection) return
      const kind: SyncRootKind = pair.remotePath === '/' ? 'unified' : 'custom'
      await koofr.migrateSyncPairToOnDemand(pair.id, selection.grantId, kind)
      await sync.refresh()
      setNotice('已切换为按需同步。原文件夹保留为完整副本，确认新位置无误后可自行删除。')
    } catch (error) {
      setNotice(commandErrorMessage(error, '切换未完成，原来的同步设置保持不变。'))
    }
  }

  const openRoot = async (pair: SyncPairSummary) => {
    try {
      await koofr.openSyncRoot(pair.id)
    } catch (error) {
      setNotice(commandErrorMessage(error, '无法在资源管理器中打开同步文件夹'))
    }
  }

  const detailSamples = detailPairId ? speedSamples.filter((sample) => sample.pairId === detailPairId).slice(-60) : []
  const detailTasks = detailPairId ? tasks[detailPairId] ?? [] : []
  const detailActiveTask = detailTasks.find((task) => task.state === 'running' || task.state === 'waiting')
    ?? detailTasks.find((task) => task.state === 'queued')
    ?? detailTasks.find((task) => task.state === 'failed')
  const detailProgress = detailPair ? completionPercent(detailPair.completedItems, detailPair.totalItems, detailPair.completedBytes, detailPair.totalBytes) ?? 0 : 0
  const detailCurrentTask = detailPair ? currentTaskSummary(detailPair) : null

  return (
    <section className="workspace sync-workspace" aria-label="同步">
      <div className="workspace__header">
        <div><h1>同步文件夹</h1><p className="workspace__description">让电脑上的文件夹与 Koofr 保持一致。按需同步会在资源管理器中显示所有文件，打开时才下载。</p></div>
        <button className="button button--primary" type="button" onClick={() => void chooseFolder()}><FolderSync size={15} strokeWidth={1.85} /> 新建同步</button>
      </div>
      <div className="sync-workspace__body">
        {notice ? <div className="notice notice--error">{notice}</div> : null}

        <div className="sync-create-card">
          <div className="sync-create-card__head">
            <div className="sync-create-card__title"><FolderSync size={15} strokeWidth={1.8} /> 创建本地文件夹同步</div>
            <p>首次同步会合并两边已有内容；同名不同内容会保留冲突副本。按需同步目前为预览功能。</p>
          </div>
          <div className="sync-create-card__grid">
            <select className="input sync-create-card__mode" value={syncMode} onChange={(event) => setSyncMode(event.target.value as SyncMode)} aria-label="同步模式">
              <option value="classic">经典同步</option>
              <option value="onDemand" disabled={nativeCapability?.supported === false}>按需同步（预览）</option>
            </select>
            {syncMode === 'onDemand' ? <select className="input sync-create-card__scope" value={rootKind} onChange={(event) => setRootKind(event.target.value as SyncRootKind)} aria-label="按需同步范围"><option value="custom">选择 Koofr 文件夹</option><option value="unified">同步整个 Koofr</option></select> : null}
            <button className={`input input--button sync-create-card__local${syncMode === 'classic' ? ' sync-create-card__local--classic' : ''}`} type="button" onClick={() => void chooseFolder()}><Folder size={15} strokeWidth={1.8} /> {localSelection?.localPath ? <span translate="no">{localSelection.localPath}</span> : '选择本地文件夹'}</button>
            {syncMode === 'onDemand' && rootKind === 'unified' && suggestedRoot ? <p className="sync-create-card__hint sync-create-card__hint--wide">“同步整个 Koofr”会同步主存储的全部内容，无法只选子文件夹。建议的本地位置：<span translate="no">{suggestedRoot}</span></p> : null}
            <select className="input sync-create-card__mount" value={mountId} onChange={(event) => setMountId(event.target.value)} aria-label="Koofr 主存储">
              <option value="">选择 Koofr 主存储</option>
              {primaryMounts.map((mount) => <option translate="no" key={mount.id} value={mount.id}>{mount.name}</option>)}
            </select>
            {syncMode === 'onDemand' && rootKind === 'unified' ? <div className="sync-remote-picker sync-remote-picker--unified"><div className="input sync-unified-target"><Cloud size={15} strokeWidth={1.8} /><span>整个 Koofr</span><code>/</code></div><button className="button" type="button" onClick={() => setRootKind('custom')}>改为选择文件夹</button></div> : <div className="sync-remote-picker" ref={remotePickerRef}>
              <div className="sync-remote-picker__row"><input className="input" value={remotePath} onChange={(event) => setRemotePath(event.target.value)} placeholder="Koofr 文件夹，例如 /My desktop sync" aria-label="Koofr 文件夹" /><button className="button" type="button" disabled={remoteBrowserBusy || !mountId} onClick={() => void browseRemote(remotePath.trim() || '/')}>{remoteBrowserBusy ? <LoaderCircle className="spin" size={14} /> : '浏览 Koofr'}</button></div>
              {remoteBrowserOpen ? <div className="sync-remote-picker__menu" role="dialog" aria-label="选择 Koofr 文件夹">
                <div className="sync-remote-picker__toolbar"><button className="sync-picker-action sync-picker-action--secondary" type="button" disabled={remoteBrowserPath === '/'} onClick={() => void browseRemote(parentPath(remoteBrowserPath))}><ArrowLeft size={14} /> 返回上一级</button><strong className="sync-remote-picker__path" translate="no" title={remoteBrowserPath}>{remoteBrowserPath}</strong><button className="sync-picker-action sync-picker-action--primary" type="button" onClick={() => chooseRemoteFolder(remoteBrowserPath)}><Check size={14} /> 选择此目录</button></div>
                <div className="sync-remote-picker__divider" />
                {remoteFolders.map((folder) => <button className="sync-remote-picker__item" type="button" key={folder.path} onClick={() => void browseRemote(folder.path)}><Folder size={15} strokeWidth={1.8} /><span translate="no">{folder.name}</span><ChevronRight size={14} /></button>)}
                {!remoteFolders.length ? <span className="sync-remote-picker__empty">当前目录没有子文件夹</span> : null}
              </div> : null}
            </div>}
            <button className="button button--primary sync-create-card__submit" type="button" disabled={busy || !localSelection || !mountId || (syncMode === 'onDemand' && nativeCapability?.supported !== true)} onClick={() => void createPair()}>{busy ? <LoaderCircle className="spin" size={15} /> : <CheckCircle2 size={15} strokeWidth={1.85} />} 确认创建</button>
          </div>
          {syncMode === 'onDemand' && nativeCapability?.supported === false ? <div className="notice notice--error">{nativeCapability.reason || '此 Windows 版本不支持按需同步。'}</div> : null}
          {syncMode === 'onDemand' ? <p className="sync-create-card__hint">创建后会启用开机启动和关闭到托盘；完全退出应用时在线文件暂时无法打开。</p> : null}
        </div>

        {pairs.length === 0 ? <div className="empty-state"><FolderSync size={22} strokeWidth={1.7} /><h2>还没有同步文件夹</h2><p>选择一个本地文件夹和 Koofr 主存储目录开始。</p></div> : <div className="sync-list">{pairs.map((pair) => {
          const active = pair.status === 'syncing' || pair.status === 'scanning' || pair.status === 'initializing'
          const percent = active ? completionPercent(pair.completedItems, pair.totalItems, pair.completedBytes, pair.totalBytes) : null
          return <article className={`sync-card sync-card--${pair.status}`} key={pair.id}>
            <div className="sync-card__icon" aria-hidden="true"><FolderSync size={16} strokeWidth={1.8} /></div>
            <div className="sync-card__body">
              <div className="sync-card__heading"><strong translate="no">{pair.localRoot}</strong><span className="sync-status"><span className="sync-status__dot" />{statusText[pair.status]}</span></div>
              <div className="sync-card__paths">Koofr 主存储 <span>↔</span> <span translate="no">{pair.remotePath}</span></div>
              <div className="sync-card__stats"><span>本地 {pair.local.fileCount} 个文件 · {formatBytes(pair.local.bytes)}</span><span>云端 {pair.remote.fileCount} 个文件 · {formatBytes(pair.remote.bytes)}</span></div>
              {pair.syncMode === 'onDemand' ? <div className="sync-card__meta"><span>按需同步 · {pair.nativeConnected ? '已连接 Windows' : 'Windows 离线'}</span><span>在线 {pair.onlineOnlyFiles} · 已下载 {pair.locallyAvailableFiles} · 始终保留 {pair.pinnedFiles}</span></div> : null}
              <div className="sync-card__meta">{active ? <><span>已完成 {pair.completedItems}/{pair.totalItems} 项 · {formatBytes(pair.completedBytes)}/{formatBytes(pair.totalBytes)}</span><span>↑ {formatRate(pair.upload.bytesPerSecond)}　↓ {formatRate(pair.download.bytesPerSecond)}</span></> : <span>最近同步：{formatTime(pair.lastSyncedAt)}</span>}{pair.issueCount > 0 ? <button className="text-button text-button--warning" type="button" onClick={() => void showDetail(pair, 'issues')}><TriangleAlert size={13} /> {pair.issueCount} 个问题</button> : null}</div>
              {percent !== null ? <div className="sync-card__progress" role="progressbar" aria-label={`${pair.localRoot} 同步进度`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)}><span style={{ width: `${percent}%` }} /></div> : null}
            </div>
            <div className="sync-card__actions"><button className="button button--subtle sync-card__details" type="button" onClick={() => void showDetail(pair)}>{'详情'}</button>{pair.syncMode === 'onDemand' ? <button className="icon-button" type="button" title="在资源管理器中打开" onClick={() => void openRoot(pair)}><FolderOpen size={15} strokeWidth={1.8} /></button> : nativeCapability?.supported ? <button className="button button--subtle" type="button" onClick={() => void migrate(pair)}>切换为按需同步</button> : null}<button className="icon-button" type="button" title={pair.paused ? '继续同步' : '暂停同步'} onClick={() => void toggle(pair)}>{pair.paused ? <Play size={15} strokeWidth={1.8} /> : <Pause size={15} strokeWidth={1.8} />}</button><button className="icon-button" type="button" title="立即同步" onClick={() => void runNow(pair)}><RefreshCw size={15} strokeWidth={1.8} /></button><button className="icon-button icon-button--danger" type="button" title="移除同步" onClick={() => setPendingRemovePair(pair)}><Trash2 size={15} strokeWidth={1.8} /></button></div>
          </article>
        })}</div>}
      </div>

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

      {detailPair ? <Modal title={`同步详情 · ${detailPair.localRoot}`} actionLabel="关闭" onClose={() => setDetailPairId(null)} extraWide>
        <div className="sync-detail-tabs"><button className={detailSection === 'overview' ? 'is-active' : ''} type="button" onClick={() => setDetailSection('overview')}>概览</button><button className={detailSection === 'issues' ? 'is-active' : ''} type="button" onClick={() => void showDetail(detailPair, 'issues')}>问题 {detailPair.issueCount > 0 ? `(${detailPair.issueCount})` : ''}</button></div>
        <div className="sync-detail-summary"><span className={`sync-status sync-status--${detailPair.status}`}><span className="sync-status__dot" />{statusText[detailPair.status]}</span><span>最近扫描：{formatTime(detailPair.lastScanAt)}</span><span>最近成功：{formatTime(detailPair.lastSyncedAt)}</span></div>
        <div className="sync-detail-paths"><span translate="no">{detailPair.localRoot}</span><span>↔</span><span>Koofr 主存储 · <span translate="no">{detailPair.remotePath}</span></span></div>
        {detailSection === 'overview' ? <>
          <div className="sync-detail-endpoints"><EndpointStats label="本地" stats={detailPair.local} /><EndpointStats label="云端" stats={detailPair.remote} /></div>
          <div className="sync-detail-cycle">
            <h3>当前任务</h3>
            <div className="sync-current-task"><strong>{detailCurrentTask?.title}</strong><span>{detailCurrentTask?.detail}</span>{detailActiveTask ? <div className="sync-current-task__file"><span>{taskOperationText[detailActiveTask.operation] ?? '同步'}</span><strong translate="no" title={detailActiveTask.displayName}>{detailActiveTask.displayName}</strong><small>{taskStateText[detailActiveTask.state]}</small></div> : null}</div>
            <div className="sync-task-queue" aria-label="当前同步任务队列"><DirectionTaskProgress label="上传" stats={detailPair.upload} /><DirectionTaskProgress label="下载" stats={detailPair.download} /></div>
            <div className="sync-card__progress" role="progressbar" aria-label="当前周期总进度" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(detailProgress)}><span style={{ width: `${detailProgress}%` }} /></div>
          </div>
          <div className="sync-task-list"><div className="sync-task-list__header"><strong>任务明细</strong><span>最近处理的文件</span></div>{detailTasks.length > 0 ? detailTasks.map((task) => <div className={`sync-task-item sync-task-item--${task.state}`} key={task.id}><span className="sync-task-item__direction" aria-hidden="true">{task.direction === 'upload' ? '↑' : task.direction === 'download' ? '↓' : '↕'}</span><strong translate="no" title={task.displayName}>{task.displayName}</strong><span>{taskOperationText[task.operation] ?? '同步'}</span><small>{task.bytes > 0 ? formatBytes(task.bytes) : '—'} · {taskStateText[task.state]}</small></div>) : <p className="sync-task-list__empty">{detailPair.status === 'scanning' || detailPair.status === 'initializing' ? '正在扫描，任务明细将在队列生成后显示。' : '当前没有可显示的同步任务。'}</p>}</div>
          <div className="sync-detail-speeds"><div><strong>当前上传速度</strong><span>{formatRate(detailPair.upload.bytesPerSecond)}</span></div><div><strong>当前下载速度</strong><span>{formatRate(detailPair.download.bytesPerSecond)}</span></div><div><strong>最近 60 秒平均</strong><span>{formatRate(detailSamples.length ? detailSamples.reduce((sum, sample) => sum + sample.uploadBytesPerSecond, 0) / detailSamples.length : 0)} ↑ · {formatRate(detailSamples.length ? detailSamples.reduce((sum, sample) => sum + sample.downloadBytesPerSecond, 0) / detailSamples.length : 0)} ↓</span></div><div><strong>最近 60 秒峰值</strong><span>{formatRate(Math.max(0, ...detailSamples.map((sample) => sample.uploadBytesPerSecond)))} ↑ · {formatRate(Math.max(0, ...detailSamples.map((sample) => sample.downloadBytesPerSecond)))} ↓</span></div></div>
          <SyncSpeedChart samples={detailSamples} />
        </> : <div ref={detailIssuesRef} className="sync-issues sync-issues--detail">{issues[detailPair.id]?.length ? issues[detailPair.id].map((issue) => <div className="sync-issue" key={issue.id}><TriangleAlert size={14} /><span>{issue.relativePath ? <><span translate="no">{issue.relativePath}</span>: </> : '同步位置：'}{issue.message}</span></div>) : <p>当前没有未解决问题。</p>}</div>}
      </Modal> : null}
    </section>
  )
}
