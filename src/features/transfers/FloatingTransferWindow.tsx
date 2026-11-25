import { useCallback, useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from 'react'
import { ArrowDownToLine, ArrowUpDown, ArrowUpToLine, Check, Cloud, FolderSync, Pause, Play } from 'lucide-react'
import { koofr } from '../../services/koofr'
import { formatRate } from '../../utils/format'
import type { FloatingTransferDirection, FloatingTransferSnapshot } from './floatingTransfer'

const INITIAL_SNAPSHOT: FloatingTransferSnapshot = {
  mode: 'idle', direction: null, percent: null, taskCount: 0, activityKind: 'none', syncPairCount: 0, syncIssueCount: 0,
  uploadBytesPerSecond: 0, downloadBytesPerSecond: 0, completionVersion: 0, language: 'system', reduceMotion: false,
  themeId: 'koofr', compactEnabled: true, clickToggleEnabled: true, doubleClickOpenEnabled: true, resumablePausedCount: 0,
}

function resolvedEnglish(language: FloatingTransferSnapshot['language']) {
  if (language === 'enUs') return true
  if (language === 'zhCn') return false
  return !navigator.language.toLowerCase().startsWith('zh')
}

function DirectionIcon({ direction, sync }: { readonly direction: FloatingTransferDirection; readonly sync: boolean }) {
  if (sync && direction === null) return <FolderSync aria-hidden="true" />
  if (direction === 'upload') return <ArrowUpToLine aria-hidden="true" />
  if (direction === 'download') return <ArrowDownToLine aria-hidden="true" />
  return <ArrowUpDown aria-hidden="true" />
}

function activeStatus(direction: FloatingTransferDirection, english: boolean) {
  if (direction === 'upload') return english ? 'Uploading' : '正在上传'
  if (direction === 'download') return english ? 'Downloading' : '正在下载'
  return english ? 'Transferring' : '正在传输'
}

export function FloatingTransferWindow() {
  const [snapshot, setSnapshot] = useState(INITIAL_SNAPSHOT)
  const [showCompleted, setShowCompleted] = useState(false)
  const previousMode = useRef(snapshot.mode)
  const previousCompletionVersion = useRef(snapshot.completionVersion)
  const completionTimer = useRef<number | null>(null)
  const clickTimer = useRef<number | null>(null)

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void koofr.onFloatingTransferSnapshot((next) => {
      if (disposed) return
      const completedNow = next.mode === 'idle' && previousMode.current !== 'idle' && next.completionVersion > previousCompletionVersion.current
      if (completionTimer.current !== null) window.clearTimeout(completionTimer.current)
      if (next.mode !== 'idle') setShowCompleted(false)
      else if (completedNow) {
        setShowCompleted(true)
        completionTimer.current = window.setTimeout(() => setShowCompleted(false), 1500)
      }
      previousMode.current = next.mode
      previousCompletionVersion.current = next.completionVersion
      setSnapshot(next)
    }).then((stopListening) => { if (disposed) stopListening(); else unlisten = stopListening })
    return () => {
      disposed = true
      unlisten?.()
      if (completionTimer.current !== null) window.clearTimeout(completionTimer.current)
      if (clickTimer.current !== null) window.clearTimeout(clickTimer.current)
    }
  }, [])

  const english = resolvedEnglish(snapshot.language)
  const roundedPercent = snapshot.percent === null ? null : Math.round(snapshot.percent)
  const completed = showCompleted && snapshot.mode === 'idle'
  const compact = snapshot.compactEnabled && !completed && snapshot.mode !== 'active' && snapshot.mode !== 'attention' && snapshot.mode !== 'offline'

  useEffect(() => { void koofr.setFloatingWindowCompact(compact, snapshot.reduceMotion).catch(() => {}) }, [compact, snapshot.reduceMotion])

  const progressWidth = completed ? 100 : snapshot.mode === 'active' ? roundedPercent ?? 0 : 0
  const status = completed
    ? (english ? 'Transfer complete' : '传输完成')
    : snapshot.mode === 'active'
      ? snapshot.activityKind === 'sync' ? (english ? 'Syncing' : '正在同步')
        : snapshot.activityKind === 'mixed' ? (english ? 'Transfer and sync' : '传输与同步中')
          : activeStatus(snapshot.direction, english)
      : snapshot.mode === 'attention' ? (english ? 'Sync needs attention' : '同步需要处理')
        : snapshot.mode === 'offline' ? (english ? 'Sync offline' : '同步等待网络')
          : snapshot.mode === 'paused' ? (english ? 'Activity paused' : snapshot.activityKind === 'sync' ? '同步已暂停' : '传输已暂停')
            : (english ? 'No activity' : '暂无传输')
  const detail = completed
    ? (english ? 'All tasks finished' : '所有任务均已完成')
    : snapshot.mode === 'idle'
      ? (english ? 'Right-click for options' : '右键查看更多操作')
      : snapshot.mode === 'paused'
        ? (english ? `${snapshot.taskCount} task(s) waiting` : `${snapshot.taskCount} 个任务等待继续`)
        : snapshot.mode === 'attention'
          ? (english ? `${snapshot.syncIssueCount} issue(s) need review` : `${snapshot.syncIssueCount} 个问题待处理`)
          : snapshot.mode === 'offline'
            ? (english ? `${snapshot.syncPairCount} sync pair(s) offline` : `${snapshot.syncPairCount} 组同步离线`)
            : roundedPercent === null
              ? (english ? `${snapshot.taskCount} task(s) · Calculating` : `${snapshot.taskCount} 个任务 · 计算中`)
              : (english ? `${snapshot.taskCount} task(s) in progress` : `${snapshot.taskCount} 个任务进行中`)
  const value = completed ? '100%' : snapshot.mode === 'active' ? roundedPercent === null ? (english ? 'Wait' : '计算') : `${roundedPercent}%` : snapshot.mode === 'paused' ? (english ? 'Paused' : '暂停') : null
  const speedDetail = snapshot.mode === 'active' ? `↑ ${formatRate(snapshot.uploadBytesPerSecond)} · ↓ ${formatRate(snapshot.downloadBytesPerSecond)}` : null

  const clearPendingClick = useCallback(() => {
    if (clickTimer.current === null) return false
    window.clearTimeout(clickTimer.current)
    clickTimer.current = null
    return true
  }, [])

  const handleMouseDown = useCallback((event: ReactMouseEvent) => {
    if (event.button !== 0) return
    const startX = event.clientX
    const startY = event.clientY
    let dragging = false
    const handleMouseMove = (moveEvent: MouseEvent) => {
      if (dragging) return
      if (Math.abs(moveEvent.clientX - startX) > 4 || Math.abs(moveEvent.clientY - startY) > 4) {
        dragging = true
        clearPendingClick()
        window.removeEventListener('mousemove', handleMouseMove)
        window.removeEventListener('mouseup', handleMouseUp)
        void koofr.startFloatingWindowDrag()
      }
    }
    const handleMouseUp = () => {
      window.removeEventListener('mousemove', handleMouseMove)
      window.removeEventListener('mouseup', handleMouseUp)
      if (dragging) return
      if (clearPendingClick()) {
        if (snapshot.doubleClickOpenEnabled) void koofr.openMainWindowFromFloating()
        else if (snapshot.clickToggleEnabled && (snapshot.mode === 'active' || snapshot.mode === 'paused')) void koofr.emitFloatingTransferAction('toggle')
        return
      }
      if (!snapshot.clickToggleEnabled && !snapshot.doubleClickOpenEnabled) return
      clickTimer.current = window.setTimeout(() => {
        clickTimer.current = null
        if (snapshot.clickToggleEnabled && (snapshot.mode === 'active' || snapshot.mode === 'paused' || snapshot.mode === 'attention' || snapshot.mode === 'offline')) void koofr.emitFloatingTransferAction('toggle')
      }, 300)
    }
    window.addEventListener('mousemove', handleMouseMove)
    window.addEventListener('mouseup', handleMouseUp)
  }, [clearPendingClick, snapshot])

  return <div className={`floating-transfer-shell floating-transfer-shell--${completed ? 'completed' : snapshot.mode}${compact ? ' floating-transfer-shell--compact' : ''}`} data-theme={snapshot.themeId} data-reduce-motion={snapshot.reduceMotion ? 'true' : 'false'} role="button" aria-label={status} title={english ? 'Click to pause or resume · Double-click to open · Right-click for options' : '单击暂停或继续 · 双击打开应用 · 右键打开菜单'} onMouseDown={handleMouseDown} onContextMenu={(event) => { event.preventDefault(); clearPendingClick(); void koofr.showFloatingContextMenu() }}>
    <span className={`floating-transfer-progress${snapshot.mode === 'active' && roundedPercent === null ? ' floating-transfer-progress--unknown' : ''}`} style={{ transform: `scaleX(${progressWidth / 100})` }} aria-hidden="true" />
    <div className="floating-transfer-content">
      <span className="floating-transfer-icon">{completed ? <Check aria-hidden="true" /> : snapshot.mode === 'active' ? <><span className="floating-transfer-icon__state"><DirectionIcon direction={snapshot.direction} sync={snapshot.activityKind === 'sync'} /></span><Pause className="floating-transfer-icon__action" aria-hidden="true" /></> : snapshot.mode === 'paused' ? <><Pause className="floating-transfer-icon__state" aria-hidden="true" /><Play className="floating-transfer-icon__action" aria-hidden="true" /></> : snapshot.mode === 'attention' || snapshot.mode === 'offline' ? <FolderSync aria-hidden="true" /> : <Cloud aria-hidden="true" />}</span>
      <span className="floating-transfer-copy"><strong>{status}</strong><small>{detail}{speedDetail ? ` · ${speedDetail}` : ''}</small></span>
      {value ? <span className="floating-transfer-value">{value}</span> : null}
    </div>
  </div>
}
