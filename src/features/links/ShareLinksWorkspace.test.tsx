import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { KoofrMount, PublicLink } from '../../types/backend'
import { publicLinks } from '../../services/publicLinks'
import { ShareLinksWorkspace } from './ShareLinksWorkspace'

vi.mock('../../services/publicLinks', () => ({
  publicLinks: {
    list: vi.fn(),
    create: vi.fn(),
    remove: vi.fn(),
  },
}))

const MOUNTS: KoofrMount[] = [{
  id: 'mount_1',
  name: 'Koofr',
  mountType: 'koofr',
  spaceTotal: 1_000,
  spaceUsed: 100,
  online: true,
  isPrimary: true,
  isShared: false,
}]

const DOWNLOAD_LINK: PublicLink = {
  id: 'download_1',
  name: 'report.pdf',
  path: '/report.pdf',
  counter: 3,
  url: 'https://app.koofr.net/links/download_1',
  shortUrl: 'https://k00.fr/report',
  hasPassword: false,
  kind: 'download',
}

describe('ShareLinksWorkspace', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(publicLinks.list).mockResolvedValue([DOWNLOAD_LINK])
    vi.mocked(publicLinks.remove).mockResolvedValue()
  })

  it('loads links for the selected storage and supports filtering', async () => {
    const user = userEvent.setup()
    render(<ShareLinksWorkspace mounts={MOUNTS} />)

    expect((await screen.findByRole('textbox', { name: 'report.pdf 的链接地址' })).getAttribute('value'))
      .toBe(DOWNLOAD_LINK.shortUrl)
    expect(publicLinks.list).toHaveBeenCalledWith('mount_1')

    await user.type(screen.getByRole('searchbox', { name: '筛选分享链接' }), 'missing')
    expect(screen.getByText('没有匹配的下载链接')).not.toBeNull()
  })

  it('requires a second click before revoking a link', async () => {
    const user = userEvent.setup()
    render(<ShareLinksWorkspace mounts={MOUNTS} />)

    await screen.findByText('report.pdf')
    await user.click(screen.getByRole('button', { name: '撤销 report.pdf' }))
    expect(publicLinks.remove).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: '确认撤销 report.pdf' }))

    await waitFor(() => expect(publicLinks.remove).toHaveBeenCalledWith('mount_1', 'download_1', 'download'))
    expect(screen.queryByText('report.pdf')).toBeNull()
  })
})
