import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { KoofrMount, RemoteFile } from '../../types/backend'

const service = vi.hoisted(() => ({
  listMounts: vi.fn(),
  listFiles: vi.fn(),
}))
vi.mock('../../services/koofr', () => ({
  commandErrorMessage: (_error: unknown, fallback: string) => fallback,
  koofr: service,
}))
import { useKoofrWorkspace } from './useKoofrWorkspace'

const mount: KoofrMount = {
  id: 'mount-1',
  name: 'Koofr',
  mountType: 'device',
  spaceTotal: 0,
  spaceUsed: 0,
  online: true,
  isPrimary: true,
  isShared: false,
}

function file(path: string): RemoteFile {
  return {
    name: path.split('/').at(-1) ?? path,
    entryType: 'file',
    modified: 0,
    size: 1,
    contentType: 'text/plain',
    hash: '',
    path,
  }
}

describe('useKoofrWorkspace', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    service.listMounts.mockResolvedValue([mount])
    service.listFiles.mockImplementation(async (_mountId: string, path: string) => [
      file(`${path === '/' ? '' : path}/listed.txt`),
    ])
  })

  it('does not pull the view back when a late refresh targets another folder', async () => {
    const { result } = renderHook(() => useKoofrWorkspace(true))
    await waitFor(() => expect(result.current.status).toBe('ready'))

    // An upload starts in /uploads and the user then browses to /photos.
    await act(async () => {
      await result.current.loadDirectory(mount.id, '/uploads')
    })
    const refreshUploadFolder = result.current.refreshIfAt
    await act(async () => {
      await result.current.loadDirectory(mount.id, '/photos')
    })
    service.listFiles.mockClear()

    // The upload finishes and asks for its folder to be refreshed.
    await act(async () => {
      await refreshUploadFolder(mount.id, '/uploads')
    })

    expect(service.listFiles).not.toHaveBeenCalled()
    expect(result.current.path).toBe('/photos')
  })

  it('refreshes in place when the finished work belongs to the open folder', async () => {
    const { result } = renderHook(() => useKoofrWorkspace(true))
    await waitFor(() => expect(result.current.status).toBe('ready'))
    await act(async () => {
      await result.current.loadDirectory(mount.id, '/uploads')
    })
    service.listFiles.mockClear()

    await act(async () => {
      await result.current.refreshIfAt(mount.id, '/uploads')
    })

    expect(service.listFiles).toHaveBeenCalledWith(mount.id, '/uploads', true)
    expect(result.current.path).toBe('/uploads')
  })

  it('refreshes the folder that is open now, not the one captured earlier', async () => {
    const { result } = renderHook(() => useKoofrWorkspace(true))
    await waitFor(() => expect(result.current.status).toBe('ready'))
    const refreshCapturedAtRoot = result.current.refreshCurrent
    await act(async () => {
      await result.current.loadDirectory(mount.id, '/photos')
    })
    service.listFiles.mockClear()

    await act(async () => {
      await refreshCapturedAtRoot()
    })

    expect(service.listFiles).toHaveBeenCalledWith(mount.id, '/photos', true)
    expect(result.current.path).toBe('/photos')
  })
})
