import {
  ArrowLeft,
  Check,
  CheckCircle2,
  ChevronRight,
  Cloud,
  Folder,
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
  SyncPairStatus,
  SyncPairSummary,
} from '../../types/backend'
import type { SyncManagerController, SyncSpeedSample } from '../../hooks/useSyncManager'

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

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MiB`
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GiB`
}

function formatRate(bytes: number) {
  return bytes > 0 ? `${formatBytes(bytes)}/s` : '—'
}

function formatTime(value: number | null) {
  return value ? new Date(value).toLocaleString('zh-CN', { hour12: false }) : '尚未完成'
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

export function SyncWorkspace({ mounts, defaultMountId, defaultRemotePath, sync }: SyncWorkspaceProps) {
  const { pairs, speedSamples } = sync
  const [issues, setIssues] = useState<Record<string, SyncIssue[]>>({})
  const [localSelection, setLocalSelection] = useState<LocalFileSelection | null>(null)
  const [mountId, setMountId] = useState(defaultMountId)
  const [remotePath, setRemotePath] = useState(defaultRemotePath || '/')
  const [remoteBrowserPath, setRemoteBrowserPath] = useState('/')
  const [remoteFolders, setRemoteFolders] = useState<RemoteFile[]>([])
  const [remoteBrowserOpen, setRemoteBrowserOpen] = useState(false)
  const [remoteBrowserBusy, setRemoteBrowserBusy] = useState(false)
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const [detailPairId, setDetailPairId] = useState<string | null>(null)
  const [detailSection, setDetailSection] = useState<'overview' | 'issues'>('overview')
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
      await koofr.createSyncPair(localSelection.grantId, mountId, remotePath.trim())
      setLocalSelection(null)
      await sync.refresh()
    } catch (error) {
      setNotice(commandErrorMessage(error, '无法创建同步，请确认远端目录可写且未重复映射'))
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

  const remove = async (pair: SyncPairSummary) => {
    if (!window.confirm(`移除“${pair.localRoot}”的同步配置？两边的文件不会被删除。`)) return
    try {
      await sync.removePair(pair.id)
    } catch (error) {
      setNotice(commandErrorMessage(error, '无法移除同步配置'))
    }
  }

  const runNow = async (pair: SyncPairSummary) => {
    try {
      await sync.runPairNow(pair.id)
    } catch (error) {
      setNotice(commandErrorMessage(error, '无法立即同步'))
    }
  }

  const detailSamples = detailPairId ? speedSamples.filter((sample) => sample.pairId === detailPairId).slice(-60) : []

  return (
    <section className="workspace sync-workspace" aria-label="同步">
      <div className="workspace__header">
        <div><p className="eyebrow">SYNC</p><h1>同步文件夹</h1><p className="workspace__description">本地与 Koofr 主存储保持双向同步，删除会进入对应回收站。</p></div>
        <button className="button button--primary" type="button" onClick={() => void chooseFolder()}><FolderSync size={17} /> 新建同步</button>
      </div>
      {notice ? <div className="notice notice--error">{notice}</div> : null}

      <div className="sync-create-card">
        <div className="sync-create-card__title"><FolderSync size={18} /> 创建本地文件夹同步</div>
        <p>首次同步会合并两边已有内容；同名不同内容会保留冲突副本。</p>
        <div className="sync-create-card__grid">
          <button className="input input--button" type="button" onClick={() => void chooseFolder()}><Cloud size={16} /> {localSelection?.localPath || '选择本地文件夹'}</button>
          <select className="input" value={mountId} onChange={(event) => setMountId(event.target.value)} aria-label="Koofr 主存储">
            <option value="">选择 Koofr 主存储</option>
            {primaryMounts.map((mount) => <option key={mount.id} value={mount.id}>{mount.name}</option>)}
          </select>
          <div className="sync-remote-picker" ref={remotePickerRef}>
            <div className="sync-remote-picker__row"><input className="input" value={remotePath} onChange={(event) => setRemotePath(event.target.value)} placeholder="远端文件夹，例如 /My desktop sync" aria-label="远端文件夹" /><button className="button" type="button" disabled={remoteBrowserBusy || !mountId} onClick={() => void browseRemote(remotePath.trim() || '/')}>{remoteBrowserBusy ? <LoaderCircle className="spin" size={15} /> : '浏览'}</button></div>
            {remoteBrowserOpen ? <div className="sync-remote-picker__menu" role="dialog" aria-label="选择 Koofr 文件夹">
              <div className="sync-remote-picker__toolbar"><button className="sync-picker-action sync-picker-action--secondary" type="button" disabled={remoteBrowserPath === '/'} onClick={() => void browseRemote(parentPath(remoteBrowserPath))}><ArrowLeft size={15} /> 返回上一级</button><strong className="sync-remote-picker__path" title={remoteBrowserPath}>{remoteBrowserPath}</strong><button className="sync-picker-action sync-picker-action--primary" type="button" onClick={() => chooseRemoteFolder(remoteBrowserPath)}><Check size={15} /> 选择此目录</button></div>
              <div className="sync-remote-picker__divider" />
              {remoteFolders.map((folder) => <button className="sync-remote-picker__item" type="button" key={folder.path} onClick={() => void browseRemote(folder.path)}><Folder size={17} /><span>{folder.name}</span><ChevronRight size={15} /></button>)}
              {!remoteFolders.length ? <span className="sync-remote-picker__empty">当前目录没有子文件夹</span> : null}
            </div> : null}
          </div>
          <button className="button button--primary" type="button" disabled={busy || !localSelection || !mountId} onClick={() => void createPair()}>{busy ? <LoaderCircle className="spin" size={16} /> : <CheckCircle2 size={16} />} 确认创建</button>
        </div>
      </div>

      {pairs.length === 0 ? <div className="empty-state"><FolderSync size={32} /><h2>还没有同步文件夹</h2><p>选择一个本地文件夹和 Koofr 主存储目录开始。</p></div> : <div className="sync-list">{pairs.map((pair) => {
        const active = pair.status === 'syncing' || pair.status === 'scanning' || pair.status === 'initializing'
        const totalBytes = pair.upload.totalBytes + pair.download.totalBytes
        const completedBytes = pair.upload.completedBytes + pair.download.completedBytes
        const percent = active && totalBytes > 0 ? Math.min(100, completedBytes / totalBytes * 100) : null
        return <article className={`sync-card sync-card--${pair.status}`} key={pair.id}>
          <div className="sync-card__icon"><FolderSync size={21} /></div>
          <div className="sync-card__body">
            <div className="sync-card__heading"><strong>{pair.localRoot}</strong><span className="sync-status"><span className="sync-status__dot" />{statusText[pair.status]}</span></div>
            <div className="sync-card__paths">Koofr 主存储 <span>→</span> {pair.remotePath}</div>
            <div className="sync-card__stats"><span>本地 {pair.local.fileCount} 个文件 · {formatBytes(pair.local.bytes)}</span><span>云端 {pair.remote.fileCount} 个文件 · {formatBytes(pair.remote.bytes)}</span></div>
            <div className="sync-card__meta">{active ? <><span>已完成 {pair.completedItems}/{pair.totalItems} 项 · {formatBytes(pair.completedBytes)}/{formatBytes(pair.totalBytes)}</span><span>↑ {formatRate(pair.upload.bytesPerSecond)}　↓ {formatRate(pair.download.bytesPerSecond)}</span></> : <span>最近同步：{formatTime(pair.lastSyncedAt)}</span>}{pair.issueCount > 0 ? <button className="text-button text-button--warning" type="button" onClick={() => void showDetail(pair, 'issues')}><TriangleAlert size={14} /> {pair.issueCount} 个问题</button> : null}</div>
            {percent !== null ? <div className="sync-card__progress"><span style={{ width: `${percent}%` }} /></div> : null}
          </div>
          <div className="sync-card__actions"><button className="button button--subtle sync-card__details" type="button" onClick={() => void showDetail(pair)}>{'详情'}</button><button className="icon-button" type="button" title={pair.paused ? '继续同步' : '暂停同步'} onClick={() => void toggle(pair)}>{pair.paused ? <Play size={17} /> : <Pause size={17} />}</button><button className="icon-button" type="button" title="立即同步" onClick={() => void runNow(pair)}><RefreshCw size={17} /></button><button className="icon-button icon-button--danger" type="button" title="移除同步配置" onClick={() => void remove(pair)}><Trash2 size={17} /></button></div>
        </article>
      })}</div>}

      {detailPair ? <Modal title={`同步详情 · ${detailPair.localRoot}`} actionLabel="关闭" onClose={() => setDetailPairId(null)} extraWide>
        <div className="sync-detail-tabs"><button className={detailSection === 'overview' ? 'is-active' : ''} type="button" onClick={() => setDetailSection('overview')}>概览</button><button className={detailSection === 'issues' ? 'is-active' : ''} type="button" onClick={() => void showDetail(detailPair, 'issues')}>问题 {detailPair.issueCount > 0 ? `(${detailPair.issueCount})` : ''}</button></div>
        <div className="sync-detail-summary"><span className={`sync-status sync-status--${detailPair.status}`}><span className="sync-status__dot" />{statusText[detailPair.status]}</span><span>最近扫描：{formatTime(detailPair.lastScanAt)}</span><span>最近成功：{formatTime(detailPair.lastSyncedAt)}</span></div>
        <div className="sync-detail-paths"><span>{detailPair.localRoot}</span><span>↔</span><span>Koofr 主存储 · {detailPair.remotePath}</span></div>
        {detailSection === 'overview' ? <>
          <div className="sync-detail-endpoints"><EndpointStats label="本地" stats={detailPair.local} /><EndpointStats label="云端" stats={detailPair.remote} /></div>
          <div className="sync-detail-cycle"><h3>当前周期</h3><div className="sync-detail-cycle__grid"><span>上传 {detailPair.upload.completedItems}/{detailPair.upload.totalItems} 项 · {formatBytes(detailPair.upload.completedBytes)}/{formatBytes(detailPair.upload.totalBytes)}</span><span>下载 {detailPair.download.completedItems}/{detailPair.download.totalItems} 项 · {formatBytes(detailPair.download.completedBytes)}/{formatBytes(detailPair.download.totalBytes)}</span></div><div className="sync-card__progress"><span style={{ width: `${detailPair.totalBytes > 0 ? Math.min(100, detailPair.completedBytes / detailPair.totalBytes * 100) : 0}%` }} /></div></div>
          <div className="sync-detail-speeds"><div><strong>当前上传</strong><span>{formatRate(detailPair.upload.bytesPerSecond)}</span></div><div><strong>当前下载</strong><span>{formatRate(detailPair.download.bytesPerSecond)}</span></div><div><strong>最近 60 秒平均</strong><span>{formatRate(detailSamples.length ? detailSamples.reduce((sum, sample) => sum + sample.uploadBytesPerSecond, 0) / detailSamples.length : 0)} ↑ · {formatRate(detailSamples.length ? detailSamples.reduce((sum, sample) => sum + sample.downloadBytesPerSecond, 0) / detailSamples.length : 0)} ↓</span></div><div><strong>最近 60 秒峰值</strong><span>{formatRate(Math.max(0, ...detailSamples.map((sample) => sample.uploadBytesPerSecond)))} ↑ · {formatRate(Math.max(0, ...detailSamples.map((sample) => sample.downloadBytesPerSecond)))} ↓</span></div></div>
          <SyncSpeedChart samples={detailSamples} />
        </> : <div ref={detailIssuesRef} className="sync-issues sync-issues--detail">{issues[detailPair.id]?.length ? issues[detailPair.id].map((issue) => <div className="sync-issue" key={issue.id}><TriangleAlert size={14} /><span>{issue.relativePath ? `${issue.relativePath}: ` : '同步根目录：'}{issue.message}</span></div>) : <p>当前没有未解决问题。</p>}</div>}
      </Modal> : null}
    </section>
  )
}
