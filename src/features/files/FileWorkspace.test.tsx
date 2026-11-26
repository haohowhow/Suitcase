import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { RemoteFile } from '../../types/backend'
import { FileWorkspace } from './FileWorkspace'

const defaultCallbacks = {
  onMountChange: vi.fn(),
  onNavigate: vi.fn(),
  onRefresh: vi.fn(),
  onCreateFolder: vi.fn(),
  onUpload: vi.fn(),
  onDownload: vi.fn(),
  onPreview: vi.fn(),
  onShare: vi.fn(),
  onRename: vi.fn(),
  onMove: vi.fn(),
  onCopy: vi.fn(),
  onDelete: vi.fn(),
}

describe('FileWorkspace', () => {
  it('triggers folder creation directly without an extra dropdown step', async () => {
    const user = userEvent.setup()
    const onCreateFolder = vi.fn()
    render(
      <FileWorkspace
        mounts={[]}
        activeMountId="mount-1"
        path="/"
        files={[]}
        loading={false}
        error=""
        lastSyncedAt={null}
        {...defaultCallbacks}
        onCreateFolder={onCreateFolder}
      />,
    )
    await user.click(screen.getByRole('button', { name: '新建文件夹' }))

    expect(onCreateFolder).toHaveBeenCalledTimes(1)
  })

  it('renders a distinct visual glyph for every supported file kind', () => {
    // Given
    const files = [
      { name: 'Projects', entryType: 'dir', contentType: '', path: '/Projects' },
      { name: 'budget.xlsx', entryType: 'file', contentType: 'application/vnd.ms-excel', path: '/budget.xlsx' },
      { name: 'brief.pdf', entryType: 'file', contentType: 'application/pdf', path: '/brief.pdf' },
      { name: 'notes.docx', entryType: 'file', contentType: 'application/msword', path: '/notes.docx' },
      { name: 'deck.pptx', entryType: 'file', contentType: 'application/vnd.ms-powerpoint', path: '/deck.pptx' },
      { name: 'cover.png', entryType: 'file', contentType: 'image/png', path: '/cover.png' },
      { name: 'clip.mp4', entryType: 'file', contentType: 'video/mp4', path: '/clip.mp4' },
      { name: 'track.mp3', entryType: 'file', contentType: 'audio/mpeg', path: '/track.mp3' },
      { name: 'source.zip', entryType: 'file', contentType: 'application/zip', path: '/source.zip' },
      { name: 'setup.exe', entryType: 'file', contentType: 'application/octet-stream', path: '/setup.exe' },
      { name: 'app.tsx', entryType: 'file', contentType: 'application/octet-stream', path: '/app.tsx' },
      { name: 'readme.md', entryType: 'file', contentType: 'text/markdown', path: '/readme.md' },
      { name: 'Inter.ttf', entryType: 'file', contentType: 'font/ttf', path: '/Inter.ttf' },
      { name: 'novel.epub', entryType: 'file', contentType: 'application/epub+zip', path: '/novel.epub' },
      { name: 'ubuntu.iso', entryType: 'file', contentType: 'application/octet-stream', path: '/ubuntu.iso' },
      { name: 'store.sqlite', entryType: 'file', contentType: 'application/octet-stream', path: '/store.sqlite' },
      { name: 'archive.bin', entryType: 'file', contentType: 'application/octet-stream', path: '/archive.bin' },
    ].map((file) => ({ ...file, modified: 0, size: 0, hash: '' })) satisfies RemoteFile[]

    // When
    const { container } = render(
      <FileWorkspace
        mounts={[]}
        activeMountId="mount-1"
        path="/"
        files={files}
        loading={false}
        error=""
        lastSyncedAt={null}
        {...defaultCallbacks}
      />,
    )

    // Then
    expect(new Set([...container.querySelectorAll('[data-file-kind]')].map((node) => node.getAttribute('data-file-kind'))))
      .toEqual(new Set([
        'folder', 'file', 'xlsx', 'image', 'pdf', 'docx', 'pptx', 'video', 'audio',
        'archive', 'executable', 'code', 'text', 'font', 'ebook', 'disk', 'database',
      ]))
  })

  it('shares the selected file instead of asking for a path', async () => {
    const user = userEvent.setup()
    const file: RemoteFile = {
      name: 'report.pdf',
      entryType: 'file',
      modified: 0,
      size: 10,
      contentType: 'application/pdf',
      hash: '',
      path: '/Documents/report.pdf',
    }
    const onShare = vi.fn()
    render(
      <FileWorkspace
        mounts={[]}
        activeMountId="mount-1"
        path="/Documents"
        files={[file]}
        loading={false}
        error=""
        lastSyncedAt={null}
        {...defaultCallbacks}
        onShare={onShare}
      />,
    )

    await user.click(screen.getByRole('checkbox', { name: '选择 report.pdf' }))
    await user.click(screen.getByRole('button', { name: '分享' }))

    expect(onShare).toHaveBeenCalledWith(file)
  })

  it('lets the user choose a compatible or resumable upload mode', async () => {
    // Given
    const user = userEvent.setup()
    const onUpload = vi.fn()
    render(
      <FileWorkspace
        mounts={[]}
        activeMountId="mount-1"
        path="/Videos"
        files={[]}
        loading={false}
        error=""
        lastSyncedAt={null}
        {...defaultCallbacks}
        onUpload={onUpload}
      />,
    )

    // When
    await user.click(screen.getByRole('button', { name: '选择上传方式' }))
    await user.click(screen.getByRole('menuitem', { name: /分卷上传大文件/ }))

    // Then
    expect(onUpload).toHaveBeenCalledWith('split')
  })

  it('previews supported files on double click and keeps download for unsupported files', async () => {
    const user = userEvent.setup()
    const previewable: RemoteFile = {
      name: 'report.pdf',
      entryType: 'file',
      modified: 0,
      size: 100,
      contentType: 'application/pdf',
      hash: '',
      path: '/report.pdf',
    }
    const unsupported: RemoteFile = {
      ...previewable,
      name: 'report.docx',
      contentType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
      path: '/report.docx',
    }
    const onPreview = vi.fn()
    const onDownload = vi.fn()
    render(
      <FileWorkspace
        mounts={[]}
        activeMountId="mount-1"
        path="/"
        files={[previewable, unsupported]}
        loading={false}
        error=""
        lastSyncedAt={null}
        {...defaultCallbacks}
        onPreview={onPreview}
        onDownload={onDownload}
      />,
    )

    await user.dblClick(screen.getByRole('button', { name: 'report.pdf' }))
    await user.dblClick(screen.getByRole('button', { name: 'report.docx' }))

    expect(onPreview).toHaveBeenCalledWith(previewable)
    expect(onDownload).toHaveBeenCalledWith(unsupported)
    expect(screen.getByRole('button', { name: '预览 report.pdf' })).not.toBeNull()
    expect(screen.getByRole('button', { name: '下载 report.pdf' })).not.toBeNull()
  })

  it('opens folders on single click, selects folders via checkbox for copy, and supports sorting', async () => {
    const user = userEvent.setup()
    const folder: RemoteFile = {
      name: 'Docs',
      entryType: 'dir',
      modified: 100,
      size: 0,
      contentType: '',
      hash: '',
      path: '/Docs',
    }
    const small: RemoteFile = {
      name: 'a.txt',
      entryType: 'file',
      modified: 100,
      size: 10,
      contentType: 'text/plain',
      hash: '',
      path: '/a.txt',
    }
    const large: RemoteFile = {
      name: 'b.txt',
      entryType: 'file',
      modified: 200,
      size: 5000,
      contentType: 'text/plain',
      hash: '',
      path: '/b.txt',
    }
    const onNavigate = vi.fn()
    const onCopy = vi.fn()
    render(
      <FileWorkspace
        mounts={[]}
        activeMountId="mount-1"
        path="/"
        files={[small, large, folder]}
        loading={false}
        error=""
        lastSyncedAt={null}
        {...defaultCallbacks}
        onNavigate={onNavigate}
        onCopy={onCopy}
      />,
    )

    await user.click(screen.getByRole('checkbox', { name: '选择 Docs' }))
    expect(onNavigate).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: '复制' }))
    expect(onCopy).toHaveBeenCalledWith([folder])

    await user.click(screen.getByRole('button', { name: 'Docs' }))
    expect(onNavigate).toHaveBeenCalledTimes(1)
    expect(onNavigate).toHaveBeenCalledWith('/Docs')

    await user.click(screen.getByRole('button', { name: '大小' }))
    const rows = screen.getAllByRole('row').slice(1)
    expect(rows[0].textContent).toContain('Docs')
    expect(rows[1].textContent).toContain('b.txt')
    expect(rows[2].textContent).toContain('a.txt')
  })
})
