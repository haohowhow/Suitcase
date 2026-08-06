import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { FloatingTransferSnapshot } from './floatingTransfer'

const mocks = vi.hoisted(() => ({
  listener: undefined as ((snapshot: FloatingTransferSnapshot) => void) | undefined,
  startFloatingWindowDrag: vi.fn(async () => {}),
  showFloatingContextMenu: vi.fn(async () => {}),
  openMainWindowFromFloating: vi.fn(async () => {}),
  emitFloatingTransferAction: vi.fn(async () => {}),
  setFloatingWindowCompact: vi.fn(async () => {}),
}))

vi.mock('../../services/koofr', () => ({
  koofr: {
    onFloatingTransferSnapshot: vi.fn(async (
      listener: (snapshot: FloatingTransferSnapshot) => void,
    ) => {
      mocks.listener = listener
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
    expect(document.querySelector('.floating-transfer-shell')?.className)
      .not.toContain('floating-transfer-shell--compact')

    act(() => mocks.listener?.({ ...ACTIVE, mode: 'paused', direction: 'upload', taskCount: 3 }))
    expect(screen.getByText('传输已暂停')).toBeTruthy()
    expect(document.querySelector('.floating-transfer-shell')?.className)
      .toContain('floating-transfer-shell--compact')

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
    const shell = document.querySelector('.floating-transfer-shell')
    expect(shell?.className).not.toContain('floating-transfer-shell--compact')
    if (!(shell instanceof HTMLElement)) return
    fireEvent.mouseDown(shell, { button: 0, clientX: 100, clientY: 100 })
    fireEvent.mouseUp(shell, { button: 0, clientX: 100, clientY: 100 })
    act(() => vi.advanceTimersByTime(300))
    expect(mocks.emitFloatingTransferAction).not.toHaveBeenCalled()
    expect(mocks.openMainWindowFromFloating).not.toHaveBeenCalled()
  })
})
