import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { AppSidebar } from './AppSidebar'

describe('AppSidebar', () => {
  it('routes primary views, toggles transfers, and navigates to places from the storage card', async () => {
    const user = userEvent.setup()
    const onSelect = vi.fn()
    const onToggleTransfers = vi.fn()
    const onSettingsClick = vi.fn()
    const onHelpClick = vi.fn()
    const onLogoutClick = vi.fn()

    render(
      <AppSidebar
        activeView="files"
        onSelect={onSelect}
        transferVisible={false}
        activeTransferCount={3}
        onToggleTransfers={onToggleTransfers}
        syncActiveCount={1}
        syncIssueCount={2}
        mountCount={3}
        onSettingsClick={onSettingsClick}
        onLogoutClick={onLogoutClick}
        onHelpClick={onHelpClick}
        storageName="Koofr"
        storageUsed={2500}
        storageTotal={10000}
      />,
    )

    expect(screen.getByLabelText('2 个同步问题')).not.toBeNull()
    expect(screen.getByRole('button', { name: '传输队列，3 项进行中' })).not.toBeNull()
    expect(screen.queryByRole('button', { name: '外观主题' })).toBeNull()

    await user.click(screen.getByRole('button', { name: '共享' }))
    expect(onSelect).toHaveBeenCalledWith('shared')

    await user.click(screen.getByRole('button', { name: '私人保险箱' }))
    expect(onSelect).toHaveBeenCalledWith('vault')

    await user.click(screen.getByRole('button', { name: '传输队列，3 项进行中' }))
    expect(onToggleTransfers).toHaveBeenCalledTimes(1)

    await user.click(screen.getByTitle('查看已连接存储'))
    expect(onSelect).toHaveBeenCalledWith('places')

    await user.click(screen.getByRole('button', { name: '设置' }))
    expect(onSettingsClick).toHaveBeenCalledTimes(1)

    await user.click(screen.getByRole('button', { name: '帮助与反馈' }))
    expect(onHelpClick).toHaveBeenCalledTimes(1)

    await user.click(screen.getByRole('button', { name: '退出登录' }))
    expect(onLogoutClick).toHaveBeenCalledTimes(1)
  })
})
