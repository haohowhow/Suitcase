import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { KoofrMount } from '../../types/backend'
import { ConnectedPlacesWorkspace } from './ConnectedPlacesWorkspace'

const MOUNTS: KoofrMount[] = [
  {
    id: 'koofr-1',
    name: 'Koofr',
    mountType: 'device',
    spaceTotal: 10_000,
    spaceUsed: 2_500,
    online: true,
    isPrimary: true,
    isShared: false,
  },
  {
    id: 'gdrive-1',
    name: 'Google Drive',
    mountType: 'third-party',
    spaceTotal: 0,
    spaceUsed: 0,
    online: false,
    isPrimary: false,
    isShared: false,
  },
]

describe('ConnectedPlacesWorkspace', () => {
  it('opens online mounts in Files and disables offline mounts', async () => {
    const user = userEvent.setup()
    const onOpenMount = vi.fn()
    render(
      <ConnectedPlacesWorkspace
        mounts={MOUNTS}
        activeMountId="koofr-1"
        loading={false}
        onOpenMount={onOpenMount}
        onRefresh={vi.fn()}
      />,
    )

    const openButtons = screen.getAllByRole('button', { name: '浏览文件' })
    expect((openButtons[0] as HTMLButtonElement).disabled).toBe(false)
    expect((openButtons[1] as HTMLButtonElement).disabled).toBe(true)

    await user.click(openButtons[0])
    expect(onOpenMount).toHaveBeenCalledWith('koofr-1')
  })
})
