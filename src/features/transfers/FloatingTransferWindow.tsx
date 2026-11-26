import { useCallback, useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from 'react'
import {
  AlertCircle,
  ArrowDownToLine,
  ArrowUpDown,
  ArrowUpToLine,
  Check,
  FolderSync,
  Pause,
  Play,
  WifiOff,
} from 'lucide-react'
import { BrandGlyph } from '../../components/BrandMark'
import { koofr, type FloatingWindowAnchor } from '../../services/koofr'
import { formatRate } from '../../utils/format'
import type { FloatingTransferDirection, FloatingTransferSnapshot } from './floatingTransfer'

const INITIAL_SNAPSHOT: FloatingTransferSnapshot = {
  mode: 'idle', direction: null, percent: null, taskCount: 0, activityKind: 'none', syncPairCount: 0, syncIssueCount: 0,
  uploadBytesPerSecond: 0, downloadBytesPerSecond: 0, completionVersion: 0, language: 'system', reduceMotion: false,
  themeId: 'koofr', compactEnabled: true, clickToggleEnabled: true, doubleClickOpenEnabled: true, resumablePausedCount: 0,
}

const RING_RADIUS = 17.5
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS

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

function formatCompactRate(bytes: number) {
  if (bytes < 1024) return `${Math.round(bytes)} B/s`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} K/s`
  return `${(bytes / 1024 / 1024).toFixed(1)} M/s`
}

function formatLiveSpeed(uploadBytesPerSecond: number, downloadBytesPerSecond: number) {
  const hasUpload = uploadBytesPerSecond > 0
  const hasDownload = downloadBytesPerSecond > 0
  if (hasUpload && hasDownload) {
    return `↑ ${formatCompactRate(uploadBytesPerSecond)} · ↓ ${formatCompactRate(downloadBytesPerSecond)}`
  }
  if (hasUpload) return `↑ ${formatRate(uploadBytesPerSecond)}`
  if (hasDownload) return `↓ ${formatRate(downloadBytesPerSecond)}`
  return null
}

export function FloatingTransferWindow() {
  const [snapshot, setSnapshot] = useState(INITIAL_SNAPSHOT)
  const [showCompleted, setShowCompleted] = useState(false)
  const [anchor, setAnchor] = useState<FloatingWindowAnchor>('right')
  const [visualCompact, setVisualCompact] = useState(true)
  const previousMode = useRef(snapshot.mode)
  const previousCompletionVersion = useRef(snapshot.completionVersion)
  const completionTimer = useRef<number | null>(null)
  const clickTimer = useRef<number | null>(null)

  useEffect(() => {
    let disposed = false
    let unlistenSnapshot: (() => void) | undefined
    let unlistenAnchor: (() => void) | undefined
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
    }).then((stopListening) => { if (disposed) stopListening(); else unlistenSnapshot = stopListening })
    void koofr.onFloatingWindowAnchor?.((nextAnchor) => {
      if (!disposed && (nextAnchor === 'left' || nextAnchor === 'right')) {
        setAnchor(nextAnchor)
      }
    }).then((stopListening) => { if (disposed) stopListening(); else unlistenAnchor = stopListening })
    return () => {
      disposed = true
      unlistenSnapshot?.()
      unlistenAnchor?.()
      if (completionTimer.current !== null) window.clearTimeout(completionTimer.current)
      if (clickTimer.current !== null) window.clearTimeout(clickTimer.current)
    }
  }, [])

  const english = resolvedEnglish(snapshot.language)
  const roundedPercent = snapshot.percent === null ? null : Math.round(snapshot.percent)
  const completed = showCompleted && snapshot.mode === 'idle'
  const compact = snapshot.compactEnabled && !completed && snapshot.mode !== 'active' && snapshot.mode !== 'attention' && snapshot.mode !== 'offline'

  useEffect(() => {
    let cancelled = false
    let raf1: number | null = null
    let raf2: number | null = null
    let timer: number | null = null

    if (!compact) {
      // Expand transparent HWND once atomically while keeping the compact shape
      // anchored in place, then morph smoothly inside the WebView at native FPS.
      void koofr.setFloatingWindowCompact(false, snapshot.reduceMotion)
        .then((nextAnchor) => {
          if (cancelled) return
          if (nextAnchor === 'left' || nextAnchor === 'right') setAnchor(nextAnchor)
          if (snapshot.reduceMotion) {
            setVisualCompact(false)
            return
          }
          raf1 = window.requestAnimationFrame(() => {
            raf2 = window.requestAnimationFrame(() => {
              if (!cancelled) setVisualCompact(false)
            })
          })
        })
        .catch(() => {
          if (!cancelled) setVisualCompact(false)
        })
    } else {
      // Collapse smoothly inside the fixed transparent canvas first, then shrink
      // the HWND atomically once settled so clicks pass through empty space.
      setVisualCompact(true)
      const settleDelay = snapshot.reduceMotion ? 0 : 320
      timer = window.setTimeout(() => {
        if (cancelled) return
        void koofr.setFloatingWindowCompact(true, snapshot.reduceMotion)
          .then((nextAnchor) => {
            if (!cancelled && (nextAnchor === 'left' || nextAnchor === 'right')) {
              setAnchor(nextAnchor)
            }
          })
          .catch(() => {})
      }, settleDelay)
    }

    return () => {
      cancelled = true
      if (raf1 !== null) window.cancelAnimationFrame(raf1)
      if (raf2 !== null) window.cancelAnimationFrame(raf2)
      if (timer !== null) window.clearTimeout(timer)
    }
  }, [compact, snapshot.reduceMotion])

  const visualMode = completed ? 'completed' : snapshot.mode
  const indeterminate = snapshot.mode === 'active' && roundedPercent === null
  const clampedPercent = completed
    ? 100
    : roundedPercent === null
      ? 0
      : Math.min(100, Math.max(0, roundedPercent))
  const showRing = completed || snapshot.mode === 'active' || (snapshot.mode === 'paused' && roundedPercent !== null)
  const ringOffset = RING_CIRCUMFERENCE * (1 - clampedPercent / 100)
  const showBarRow = completed || snapshot.mode === 'active' || (snapshot.mode === 'paused' && roundedPercent !== null)

  const status = completed
    ? (english ? 'Transfer complete' : '传输完成')
    : snapshot.mode === 'active'
      ? snapshot.activityKind === 'sync' ? (english ? 'Syncing' : '正在同步')
        : snapshot.activityKind === 'mixed' ? (english ? 'Transfer & sync' : '传输与同步中')
          : activeStatus(snapshot.direction, english)
      : snapshot.mode === 'attention' ? (english ? 'Sync needs attention' : '同步需要处理')
        : snapshot.mode === 'offline' ? (english ? 'Sync offline' : '同步等待网络')
          : snapshot.mode === 'paused' ? (english ? 'Activity paused' : snapshot.activityKind === 'sync' ? '同步已暂停' : '传输已暂停')
            : (english ? 'No activity' : '暂无传输')

  const taskBadge = !completed && (snapshot.mode === 'active' || snapshot.mode === 'paused') && snapshot.taskCount > 0
    ? (english ? (snapshot.taskCount === 1 ? '1 task' : `${snapshot.taskCount} tasks`) : `${snapshot.taskCount} 项`)
    : null

  const speedText = snapshot.mode === 'active'
    ? formatLiveSpeed(snapshot.uploadBytesPerSecond, snapshot.downloadBytesPerSecond)
    : null

  const detail = completed
    ? (english ? 'All tasks finished' : '全部任务已完成')
    : snapshot.mode === 'idle'
      ? (english ? 'Double-click to open · Right-click menu' : '双击打开应用 · 右键菜单')
      : snapshot.mode === 'paused'
        ? (snapshot.clickToggleEnabled
          ? (english ? `${snapshot.taskCount} waiting · Click to resume` : `${snapshot.taskCount} 项待继续 · 单击恢复`)
          : (english ? `${snapshot.taskCount} task(s) waiting` : `${snapshot.taskCount} 个任务等待继续`))
        : snapshot.mode === 'attention'
          ? (english ? `${snapshot.syncIssueCount} issue(s) need review` : `${snapshot.syncIssueCount} 个问题待处理`)
          : snapshot.mode === 'offline'
            ? (english ? `${snapshot.syncPairCount} sync pair(s) offline` : `${snapshot.syncPairCount} 组同步离线`)
            : roundedPercent === null
              ? (english ? 'Preparing…' : '计算进度…')
              : null

  const value = completed
    ? '100%'
    : snapshot.mode === 'active'
      ? (roundedPercent === null ? (english ? 'Wait' : '计算中') : `${roundedPercent}%`)
      : snapshot.mode === 'paused'
        ? (roundedPercent === null ? (english ? 'Paused' : '已暂停') : `${roundedPercent}%`)
        : snapshot.mode === 'attention' && snapshot.syncIssueCount > 0
          ? `${snapshot.syncIssueCount}`
          : null

  const barMeta = completed
    ? (english ? 'Done' : '已完成')
    : snapshot.mode === 'active'
      ? (speedText ?? (roundedPercent === null ? (english ? 'Preparing' : '准备中') : null))
      : snapshot.mode === 'paused'
        ? (snapshot.clickToggleEnabled ? (english ? 'Click to resume' : '单击继续') : (english ? 'Paused' : '已暂停'))
        : null

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

  const canHoverToggle = snapshot.clickToggleEnabled && (snapshot.mode === 'active' || snapshot.mode === 'paused')

  return (
    <div className="floating-window-stage" data-anchor={anchor}>
      <div
        className={`floating-transfer-shell floating-transfer-shell--${visualMode}${visualCompact ? ' floating-transfer-shell--compact' : ''}`}
        data-theme={snapshot.themeId}
        data-reduce-motion={snapshot.reduceMotion ? 'true' : 'false'}
        data-click-toggle={canHoverToggle ? 'true' : 'false'}
        role="button"
        aria-label={value ? `${status} ${value}` : status}
        title={english ? 'Click to pause or resume · Double-click to open · Right-click for options' : '单击暂停或继续 · 双击打开应用 · 右键打开菜单'}
        onMouseDown={handleMouseDown}
        onContextMenu={(event) => {
          event.preventDefault()
          clearPendingClick()
          void koofr.showFloatingContextMenu()
        }}
      >
        <div className="floating-transfer-content">
          <span className={`floating-transfer-dial${showRing ? ' floating-transfer-dial--ringed' : ''}`}>
            {showRing ? (
              <svg
                className={`floating-transfer-ring${indeterminate ? ' floating-transfer-ring--indeterminate' : ''}`}
                viewBox="0 0 40 40"
                aria-hidden="true"
              >
                <circle
                  className="floating-transfer-ring__track"
                  cx="20"
                  cy="20"
                  r={RING_RADIUS}
                />
                <circle
                  className="floating-transfer-ring__fill"
                  cx="20"
                  cy="20"
                  r={RING_RADIUS}
                  strokeDasharray={indeterminate ? '28 82' : `${RING_CIRCUMFERENCE}`}
                  strokeDashoffset={indeterminate ? 0 : ringOffset}
                />
              </svg>
            ) : null}
            <span className="floating-transfer-icon">
              {completed ? (
                <Check aria-hidden="true" />
              ) : snapshot.mode === 'active' ? (
                <>
                  <span className="floating-transfer-icon__state">
                    <DirectionIcon direction={snapshot.direction} sync={snapshot.activityKind === 'sync'} />
                  </span>
                  <Pause className="floating-transfer-icon__action" aria-hidden="true" />
                </>
              ) : snapshot.mode === 'paused' ? (
                <>
                  <Pause className="floating-transfer-icon__state" aria-hidden="true" />
                  <Play className="floating-transfer-icon__action" aria-hidden="true" />
                </>
              ) : snapshot.mode === 'attention' ? (
                <AlertCircle aria-hidden="true" />
              ) : snapshot.mode === 'offline' ? (
                <WifiOff aria-hidden="true" />
              ) : (
                <BrandGlyph className="floating-transfer-brand" />
              )}
            </span>
          </span>

          <div className="floating-transfer-copy">
            <div className="floating-transfer-header">
              <span className="floating-transfer-heading">
                <strong>{status}</strong>
                {taskBadge ? <span className="floating-transfer-count">{taskBadge}</span> : null}
              </span>
              {value ? <span className="floating-transfer-value">{value}</span> : null}
            </div>

            {showBarRow ? (
              <div className="floating-transfer-telemetry">
                <span className="floating-transfer-bar" aria-hidden="true">
                  <span
                    className={`floating-transfer-progress${indeterminate ? ' floating-transfer-progress--unknown' : ''}`}
                    style={indeterminate ? undefined : { transform: `scaleX(${clampedPercent / 100})` }}
                  />
                </span>
                {barMeta ? <small className="floating-transfer-meta">{barMeta}</small> : null}
              </div>
            ) : detail ? (
              <small className="floating-transfer-detail">{detail}</small>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  )
}
