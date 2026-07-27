import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { FloatingTransferSnapshot } from './floatingTransfer'

const mocks = vi.hoisted(() => ({
  listener: undefined as ((snapshot: FloatingTransferSnapshot) => void) | undefined,
  startFloatingWindowDrag: vi.fn(async () => {}),
  showFloatingContextMenu: vi.fn(async () => {}),
  openMainWindowFromFloating: vi.fn(async () => {}),
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
  },
}))

import { FloatingTransferWindow } from './FloatingTransferWindow'

const ACTIVE: FloatingTransferSnapshot = {
  mode: 'active',
  direction: 'mixed',
  percent: 62.4,
  taskCount: 2,
  completionVersion: 0,
  language: 'zhCn',
  reduceMotion: false,
  themeId: 'ocean',
}

describe('FloatingTransferWindow', () => {
  beforeEach(() => {
    mocks.listener = undefined
    mocks.startFloatingWindowDrag.mockClear()
    mocks.showFloatingContextMenu.mockClear()
    mocks.openMainWindowFromFloating.mockClear()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('renders aggregate progress, opens on click, drags on move, and opens the context menu', async () => {
    render(<FloatingTransferWindow />)
    await act(async () => {})
    act(() => mocks.listener?.(ACTIVE))

    expect(screen.getByText('正在传输')).toBeTruthy()
    expect(screen.getByText('62%')).toBeTruthy()
    expect(screen.getByText('2 个任务进行中')).toBeTruthy()
    expect(document.querySelector('.floating-transfer-shell')?.getAttribute('data-theme'))
      .toBe('ocean')

    const shell = document.querySelector('.floating-transfer-shell')
    expect(shell).toBeInstanceOf(HTMLElement)
    if (!(shell instanceof HTMLElement)) return

    // Click without movement → opens main window
    fireEvent.mouseDown(shell, { button: 0, clientX: 100, clientY: 100 })
    fireEvent.mouseUp(shell, { button: 0, clientX: 100, clientY: 100 })
    expect(mocks.openMainWindowFromFloating).toHaveBeenCalledOnce()
    expect(mocks.startFloatingWindowDrag).not.toHaveBeenCalled()

    // Right-click → context menu
    fireEvent.mouseDown(shell, { button: 2 })
    expect(mocks.startFloatingWindowDrag).not.toHaveBeenCalled()
    fireEvent.contextMenu(shell)
    expect(mocks.showFloatingContextMenu).toHaveBeenCalledOnce()

    // Drag beyond threshold → starts window drag
    fireEvent.mouseDown(shell, { button: 0, clientX: 100, clientY: 100 })
    fireEvent.mouseMove(shell, { button: 0, clientX: 110, clientY: 100 })
    expect(mocks.startFloatingWindowDrag).toHaveBeenCalledOnce()

    expect(document.querySelector('.floating-transfer-drag')).toBeNull()
  })

  it('shows an interruptible short completion state before becoming idle', async () => {
    vi.useFakeTimers()
    render(<FloatingTransferWindow />)
    await act(async () => {})
    act(() => mocks.listener?.(ACTIVE))
    act(() => mocks.listener?.({
      ...ACTIVE,
      mode: 'idle',
      direction: null,
      percent: null,
      taskCount: 0,
      completionVersion: 1,
      reduceMotion: true,
    }))

    expect(screen.getByText('传输完成')).toBeTruthy()
    expect(document.querySelector('.floating-transfer-shell')?.getAttribute('data-reduce-motion'))
      .toBe('true')

    act(() => vi.advanceTimersByTime(1500))
    expect(screen.getByText('暂无传输')).toBeTruthy()

    act(() => mocks.listener?.(ACTIVE))
    expect(screen.getByText('62%')).toBeTruthy()
  })

  it('shows informative paused and idle states instead of an empty surface', async () => {
    render(<FloatingTransferWindow />)
    await act(async () => {})
    act(() => mocks.listener?.({
      ...ACTIVE,
      mode: 'idle',
      direction: null,
      percent: null,
      taskCount: 0,
    }))

    expect(screen.getByText('暂无传输')).toBeTruthy()
    expect(screen.getByText('右键查看更多操作')).toBeTruthy()

    act(() => mocks.listener?.({
      ...ACTIVE,
      mode: 'paused',
      direction: 'upload',
      percent: 34,
      taskCount: 3,
    }))

    expect(screen.getByText('传输已暂停')).toBeTruthy()
    expect(screen.getByText('3 个任务等待继续')).toBeTruthy()
    expect(screen.getByText('暂停')).toBeTruthy()
  })

  it('labels unknown progress as calculating without drawing a fake percentage', async () => {
    render(<FloatingTransferWindow />)
    await act(async () => {})
    act(() => mocks.listener?.({
      ...ACTIVE,
      direction: 'download',
      percent: null,
      taskCount: 1,
    }))

    expect(screen.getByText('正在下载')).toBeTruthy()
    expect(screen.getByText('1 个任务 · 计算中')).toBeTruthy()
    expect(screen.getByText('计算')).toBeTruthy()
    expect(document.querySelector('.floating-transfer-progress')?.getAttribute('style'))
      .toContain('scaleX(0)')
  })
})
