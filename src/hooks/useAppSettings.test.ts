import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AppSettings } from '../types/backend'

const service = vi.hoisted(() => ({
  updateNotificationSettings: vi.fn(),
}))
vi.mock('../services/koofr', () => ({
  commandErrorMessage: (_error: unknown, fallback: string) => fallback,
  ensureNotificationPermission: vi.fn(async () => true),
  koofr: service,
}))
import { useAppSettings } from './useAppSettings'

const settings = {
  notifyOnComplete: true,
  notifySound: false,
  trayBadge: true,
  floatingWindowEnabled: false,
  floatingWindowPrompted: false,
  floatingWindowCompactEnabled: true,
  floatingWindowClickToggleEnabled: true,
  floatingWindowDoubleClickOpenEnabled: true,
} as unknown as AppSettings

describe('useAppSettings floating window prompt', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('reports a saved answer so the caller can close the prompt', async () => {
    service.updateNotificationSettings.mockResolvedValue({
      ...settings,
      floatingWindowPrompted: true,
    })
    const showNotice = vi.fn()
    const { result } = renderHook(() => useAppSettings(showNotice))
    act(() => result.current.setSettings(settings))

    let saved = false
    await act(async () => {
      saved = await result.current.answerFloatingWindowPrompt(false)
    })

    expect(saved).toBe(true)
    expect(service.updateNotificationSettings).toHaveBeenCalledWith(
      true, false, true, false, true, true, true, true,
    )
    expect(result.current.settings?.floatingWindowPrompted).toBe(true)
    expect(result.current.settingsBusy).toBe(false)
  })

  it('reports a failed save and shows the error outside the settings panel', async () => {
    service.updateNotificationSettings.mockRejectedValue(new Error('disk full'))
    const showNotice = vi.fn()
    const { result } = renderHook(() => useAppSettings(showNotice))
    act(() => result.current.setSettings(settings))

    let saved = true
    await act(async () => {
      saved = await result.current.answerFloatingWindowPrompt(true)
    })

    expect(saved).toBe(false)
    expect(showNotice).toHaveBeenCalledWith('无法保存悬浮窗设置。')
    expect(result.current.settingsBusy).toBe(false)
  })
})
