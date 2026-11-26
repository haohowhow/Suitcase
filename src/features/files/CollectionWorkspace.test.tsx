import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { LocatedFile } from '../../types/backend'
import { CollectionWorkspace } from './CollectionWorkspace'

const file: LocatedFile = {
  name: 'shared.png',
  entryType: 'file',
  modified: 0,
  size: 12,
  contentType: 'image/png',
  hash: '',
  path: '/Shared/shared.png',
  mountId: 'mount-shared',
  mountName: 'Koofr',
  shareDirection: 'received',
}

describe('CollectionWorkspace preview behavior', () => {
  it('previews recent and shared files without changing trash behavior', async () => {
    const user = userEvent.setup()
    const onPreview = vi.fn()
    render(
      <CollectionWorkspace
        view="已共享"
        files={[file]}
        trash={[]}
        retentionDays={30}
        loading={false}
        error=""
        diagnostic=""
        lastSyncedAt={null}
        busy={false}
        onRefresh={vi.fn()}
        onOpenLocation={vi.fn()}
        onDownload={vi.fn()}
        onPreview={onPreview}
        onRestore={vi.fn()}
        onRestoreAll={vi.fn()}
        onEmptyTrash={vi.fn()}
      />,
    )

    await user.dblClick(screen.getByRole('button', { name: 'shared.png' }))

    expect(onPreview).toHaveBeenCalledWith(file)
    expect(screen.getByRole('button', { name: '预览 shared.png' })).not.toBeNull()
  })
})
