import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { FloatingTransferSnapshot } from './floatingTransfer'

const mocks = vi.hoisted(() => ({
  listener: undefined as ((snapshot: FloatingTransferSnapshot) => void) | undefined,
  anchorListener: undefined as ((anchor: 'left' | 'right') => void) | undefined,
  startFloatingWindowDrag: vi.fn(async () => {}),
  showFloatingContextMenu: vi.fn(async () => {}),
  openMainWindowFromFloating: vi.fn(async () => {}),
  emitFloatingTransferAction: vi.fn(async () => {}),
  setFloatingWindowCompact: vi.fn(async (): Promise<'left' | 'right'> => 'right'),
}))

vi.mock('../../services/koofr', () => ({
  koofr: {
    onFloatingTransferSnapshot: vi.fn(async (
      listener: (snapshot: FloatingTransferSnapshot) => void,
    ) => {
      mocks.listener = listener
      return () => {}
    }),
    onFloatingWindowAnchor: vi.fn(async (
      listener: (anchor: 'left' | 'right') => void,
    ) => {
      mocks.anchorListener = listener
      return () => {}
    }),
    startFloatingWindowDrag: mocks.startFloatingWindowDrag,
    showFloatingContextMenu: mocks.showFloatingContextMenu,
    openMainWindowFromFloating: mocks.openMainWindowFromFloating,
    emitFloatingTransferAction: mocks.emitFloatingTransferAction,
    setFloatingWindowCompact: mocks.setFloatingWindowCompact,
  },
}))

import { FloatingTransferWindow } from './FloatingTransferWindow'

const ACTIVE: FloatingTransferSnapshot = {
  mode: 'active',
  direction: 'mixed',
  percent: 62.4,
  taskCount: 2,
  activityKind: 'transfer',
  syncPairCount: 0,
  syncIssueCount: 0,
  uploadBytesPerSecond: 0,
  downloadBytesPerSecond: 0,
  completionVersion: 0,
  language: 'zhCn',
  reduceMotion: false,
  themeId: 'ocean',
  compactEnabled: true,
  clickToggleEnabled: true,
  doubleClickOpenEnabled: true,
  resumablePausedCount: 0,
}

describe('FloatingTransferWindow', () => {
  beforeEach(() => {
    mocks.listener = undefined
    Object.values(mocks).forEach((value) => {
      if (typeof value === 'function' && 'mockClear' in value) value.mockClear()
    })
  })

  afterEach(() => vi.useRealTimers())

  it('renders aggregate progress and distinguishes single click, double click, drag, and right click', async () => {
    vi.useFakeTimers()
    render(<FloatingTransferWindow />)
    await act(async () => {})
    act(() => mocks.listener?.(ACTIVE))

    expect(screen.getByText('正在传输')).toBeTruthy()
    expect(screen.getByText('62%')).toBeTruthy()
    const shell = document.querySelector('.floating-transfer-shell')
    expect(shell).toBeInstanceOf(HTMLElement)
    if (!(shell instanceof HTMLElement)) return

    fireEvent.mouseDown(shell, { button: 0, clientX: 100, clientY: 100 })
    fireEvent.mouseUp(shell, { button: 0, clientX: 100, clientY: 100 })
    act(() => vi.advanceTimersByTime(300))
    expect(mocks.emitFloatingTransferAction).toHaveBeenCalledWith('toggle')
    expect(mocks.openMainWindowFromFloating).not.toHaveBeenCalled()

    fireEvent.mouseDown(shell, { button: 0, clientX: 100, clientY: 100 })
    fireEvent.mouseUp(shell, { button: 0, clientX: 100, clientY: 100 })
    fireEvent.mouseDown(shell, { button: 0, clientX: 100, clientY: 100 })
    fireEvent.mouseUp(shell, { button: 0, clientX: 100, clientY: 100 })
    expect(mocks.openMainWindowFromFloating).toHaveBeenCalledOnce()

    fireEvent.mouseDown(shell, { button: 0, clientX: 100, clientY: 100 })
    fireEvent.mouseMove(shell, { button: 0, clientX: 110, clientY: 100 })
    expect(mocks.startFloatingWindowDrag).toHaveBeenCalledOnce()

    fireEvent.contextMenu(shell)
    expect(mocks.showFloatingContextMenu).toHaveBeenCalledOnce()
  })

  it('uses a compact surface for idle and paused states, after completion feedback', async () => {
    vi.useFakeTimers()
    render(<FloatingTransferWindow />)
    await act(async () => {})
    act(() => mocks.listener?.(ACTIVE))
    await act(async () => {
      await Promise.resolve()
      vi.advanceTimersByTime(40)
    })
    expect(document.querySelector('.floating-transfer-shell')?.className)
      .not.toContain('floating-transfer-shell--compact')

    act(() => mocks.listener?.({ ...ACTIVE, mode: 'paused', direction: 'upload', taskCount: 3 }))
    expect(screen.getByText('传输已暂停')).toBeTruthy()
    expect(document.querySelector('.floating-transfer-shell')?.className)
      .toContain('floating-transfer-shell--compact')
    // Waits for the CSS collapse morph before shrinking the native HWND.
    expect(mocks.setFloatingWindowCompact).not.toHaveBeenCalledWith(true, false)
    await act(async () => {
      vi.advanceTimersByTime(320)
    })
    expect(mocks.setFloatingWindowCompact).toHaveBeenCalledWith(true, false)

    act(() => mocks.listener?.({ ...ACTIVE, mode: 'idle', direction: null, percent: null, taskCount: 0, completionVersion: 1 }))
    expect(screen.getByText('传输完成')).toBeTruthy()
    act(() => vi.advanceTimersByTime(1500))
    expect(screen.getByText('暂无传输')).toBeTruthy()
    expect(document.querySelector('.floating-transfer-shell')?.className)
      .toContain('floating-transfer-shell--compact')
  })

  it('honors disabled compact and click settings', async () => {
    vi.useFakeTimers()
    render(<FloatingTransferWindow />)
    await act(async () => {})
    act(() => mocks.listener?.({
      ...ACTIVE,
      mode: 'paused',
      compactEnabled: false,
      clickToggleEnabled: false,
      doubleClickOpenEnabled: false,
    }))
    await act(async () => {
      await Promise.resolve()
      vi.advanceTimersByTime(40)
    })
    const shell = document.querySelector('.floating-transfer-shell')
    expect(shell?.className).not.toContain('floating-transfer-shell--compact')
    if (!(shell instanceof HTMLElement)) return
    fireEvent.mouseDown(shell, { button: 0, clientX: 100, clientY: 100 })
    fireEvent.mouseUp(shell, { button: 0, clientX: 100, clientY: 100 })
    act(() => vi.advanceTimersByTime(300))
    expect(mocks.emitFloatingTransferAction).not.toHaveBeenCalled()
    expect(mocks.openMainWindowFromFloating).not.toHaveBeenCalled()
    expect(shell.getAttribute('data-click-toggle')).toBe('false')
  })

  it('shows only active transfer speed directions and the Suitcase mark when idle', async () => {
    render(<FloatingTransferWindow />)
    await act(async () => {})
    expect(document.querySelector('.floating-transfer-brand')).toBeTruthy()

    act(() => mocks.listener?.({
      ...ACTIVE,
      direction: 'download',
      uploadBytesPerSecond: 0,
      downloadBytesPerSecond: 2 * 1024 * 1024,
    }))
    expect(screen.getByText('↓ 2.00 MB/s')).toBeTruthy()
    expect(document.querySelector('.floating-transfer-ring')).toBeTruthy()
  })
})
