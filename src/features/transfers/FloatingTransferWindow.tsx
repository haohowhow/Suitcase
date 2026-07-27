import { useCallback, useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from 'react'
import {
  ArrowDownToLine,
  ArrowUpDown,
  ArrowUpToLine,
  Check,
  Cloud,
  Pause,
} from 'lucide-react'

import { koofr } from '../../services/koofr'
import type {
  FloatingTransferDirection,
  FloatingTransferSnapshot,
} from './floatingTransfer'

const INITIAL_SNAPSHOT: FloatingTransferSnapshot = {
  mode: 'idle',
  direction: null,
  percent: null,
  taskCount: 0,
  completionVersion: 0,
  language: 'system',
  reduceMotion: false,
  themeId: 'koofr',
}

function resolvedEnglish(language: FloatingTransferSnapshot['language']) {
  if (language === 'enUs') return true
  if (language === 'zhCn') return false
  return !navigator.language.toLowerCase().startsWith('zh')
}

function DirectionIcon({ direction }: { readonly direction: FloatingTransferDirection }) {
  if (direction === 'upload') return <ArrowUpToLine aria-hidden="true" />
  if (direction === 'download') return <ArrowDownToLine aria-hidden="true" />
  return <ArrowUpDown aria-hidden="true" />
}

function activeStatus(
  direction: FloatingTransferDirection,
  english: boolean,
) {
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

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void koofr.onFloatingTransferSnapshot((next) => {
      if (disposed) return
      const completedNow = next.mode === 'idle'
        && previousMode.current !== 'idle'
        && next.completionVersion > previousCompletionVersion.current
      if (completionTimer.current !== null) {
        window.clearTimeout(completionTimer.current)
        completionTimer.current = null
      }
      if (next.mode !== 'idle') {
        setShowCompleted(false)
      } else if (completedNow) {
        setShowCompleted(true)
        completionTimer.current = window.setTimeout(() => {
          setShowCompleted(false)
          completionTimer.current = null
        }, 1500)
      }
      previousMode.current = next.mode
      previousCompletionVersion.current = next.completionVersion
      setSnapshot(next)
    }).then((stopListening) => {
      if (disposed) stopListening()
      else unlisten = stopListening
    })
    return () => {
      disposed = true
      unlisten?.()
      if (completionTimer.current !== null) {
        window.clearTimeout(completionTimer.current)
      }
    }
  }, [])

  const english = resolvedEnglish(snapshot.language)
  const roundedPercent = snapshot.percent === null ? null : Math.round(snapshot.percent)
  const completed = showCompleted && snapshot.mode === 'idle'
  const progressWidth = completed
    ? 100
    : snapshot.mode === 'active' ? roundedPercent ?? 0 : 0
  const status = completed
    ? (english ? 'Transfer complete' : '传输完成')
    : snapshot.mode === 'active'
      ? activeStatus(snapshot.direction, english)
      : snapshot.mode === 'paused'
        ? (english ? 'Transfers paused' : '传输已暂停')
        : (english ? 'No transfers' : '暂无传输')
  const detail = completed
    ? (english ? 'All tasks finished' : '所有任务均已完成')
    : snapshot.mode === 'idle'
      ? (english ? 'Right-click for options' : '右键查看更多操作')
      : snapshot.mode === 'paused'
        ? english
          ? `${snapshot.taskCount} ${snapshot.taskCount === 1 ? 'task' : 'tasks'} waiting`
          : `${snapshot.taskCount} 个任务等待继续`
        : roundedPercent === null
          ? english
            ? `${snapshot.taskCount} ${snapshot.taskCount === 1 ? 'task' : 'tasks'} · Calculating`
            : `${snapshot.taskCount} 个任务 · 计算中`
          : english
            ? `${snapshot.taskCount} ${snapshot.taskCount === 1 ? 'task' : 'tasks'} in progress`
            : `${snapshot.taskCount} 个任务进行中`
  const value = completed
    ? '100%'
    : snapshot.mode === 'active'
      ? roundedPercent === null ? (english ? 'Wait' : '计算') : `${roundedPercent}%`
      : snapshot.mode === 'paused'
        ? (english ? 'Paused' : '暂停')
        : null

  const handleMouseDown = useCallback(
    (event: ReactMouseEvent) => {
      if (event.button !== 0) return
      const startX = event.clientX
      const startY = event.clientY
      let dragging = false
      const handleMouseMove = (moveEvent: MouseEvent) => {
        if (dragging) return
        if (
          Math.abs(moveEvent.clientX - startX) > 4 ||
          Math.abs(moveEvent.clientY - startY) > 4
        ) {
          dragging = true
          window.removeEventListener('mousemove', handleMouseMove)
          window.removeEventListener('mouseup', handleMouseUp)
          void koofr.startFloatingWindowDrag()
        }
      }
      const handleMouseUp = () => {
        window.removeEventListener('mousemove', handleMouseMove)
        window.removeEventListener('mouseup', handleMouseUp)
        if (!dragging) {
          void koofr.openMainWindowFromFloating()
        }
      }
      window.addEventListener('mousemove', handleMouseMove)
      window.addEventListener('mouseup', handleMouseUp)
    },
    [],
  )

  return (
    <div
      className={`floating-transfer-shell floating-transfer-shell--${completed ? 'completed' : snapshot.mode}`}
      data-theme={snapshot.themeId}
      data-reduce-motion={snapshot.reduceMotion ? 'true' : 'false'}
      role="status"
      aria-live="polite"
      title={english ? 'Click to open · Right-click for options' : '单击打开面板 · 右键打开菜单'}
      onMouseDown={handleMouseDown}
      onContextMenu={(event) => {
        event.preventDefault()
        void koofr.showFloatingContextMenu()
      }}
    >
      <span
        className={`floating-transfer-progress${snapshot.mode === 'active' && roundedPercent === null ? ' floating-transfer-progress--unknown' : ''}`}
        style={{ transform: `scaleX(${progressWidth / 100})` }}
        aria-hidden="true"
      />
      <div className="floating-transfer-content">
        <span className="floating-transfer-icon">
          {completed ? <Check aria-hidden="true" /> : snapshot.mode === 'active' ? (
            <DirectionIcon direction={snapshot.direction} />
          ) : snapshot.mode === 'paused' ? (
            <Pause aria-hidden="true" />
          ) : (
            <Cloud aria-hidden="true" />
          )}
        </span>
        <span className="floating-transfer-copy">
          <strong>{status}</strong>
          <small>{detail}</small>
        </span>
        {value ? <span className="floating-transfer-value">{value}</span> : null}
      </div>
    </div>
  )
}
