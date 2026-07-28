import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { getDocument } from 'pdfjs-dist'
import { StrictMode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { koofr } from '../../services/koofr'
import type { FilePreviewInfo } from '../../types/backend'
import { FilePreviewModal, sanitizeSvgForPreview } from './FilePreviewModal'

vi.mock('pdfjs-dist', () => ({
  GlobalWorkerOptions: {},
  PDFDataRangeTransport: class {
    onDataRange() {}
    abort() {}
  },
  getDocument: vi.fn(),
}))

const video: FilePreviewInfo = {
  sessionId: 'test-session-id',
  fileSize: 10_000_000,
  contentType: 'video/mp4',
  kind: 'video',
  decoder: 'native',
  limits: {
    maxBytes: null,
    textBytes: null,
    maxPixels: null,
  },
}

describe('FilePreviewModal', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders media and preserves the session during the StrictMode probe', () => {
    const close = vi.spyOn(koofr, 'closeFilePreview').mockResolvedValue(undefined)
    render(<StrictMode><FilePreviewModal previewInfo={video} fileName="clip.mp4" onClose={vi.fn()} onDownload={vi.fn()} /></StrictMode>)
    expect(document.querySelector('video')?.getAttribute('src')).toContain('test-session-id')
    expect(close).not.toHaveBeenCalled()
    close.mockRestore()
  })

  it('closes from the button and offers download', async () => {
    const user = userEvent.setup()
    const onClose = vi.fn()
    const onDownload = vi.fn()
    render(<FilePreviewModal previewInfo={video} fileName="clip.mp4" onClose={onClose} onDownload={onDownload} />)
    await user.click(screen.getByRole('button', { name: '下载' }))
    await user.click(screen.getByRole('button', { name: '关闭' }))
    expect(onDownload).toHaveBeenCalledOnce()
    expect(onClose).toHaveBeenCalledOnce()
  })

  it('shows a safe fallback when media decoding fails', () => {
    render(<FilePreviewModal previewInfo={video} fileName="bad.mkv" onClose={vi.fn()} onDownload={vi.fn()} />)
    const element = document.querySelector('video')
    if (element) fireEvent.error(element)
    expect(screen.getByText(/无法播放此格式/)).not.toBeNull()
  })

  it('shows text truncation and lets the user correct the encoding', async () => {
    const user = userEvent.setup()
    const read = vi.spyOn(koofr, 'readPreviewText').mockResolvedValue({
      text: '測試內容',
      encoding: 'Big5',
      truncated: true,
      hadDecodeErrors: false,
    })
    const textInfo: FilePreviewInfo = {
      ...video,
      kind: 'text',
      contentType: 'text/plain',
      decoder: 'text',
      limits: { maxBytes: null, textBytes: 5 * 1024 * 1024, maxPixels: null },
    }
    render(<FilePreviewModal previewInfo={textInfo} fileName="notes.txt" onClose={vi.fn()} onDownload={vi.fn()} />)

    await user.selectOptions(screen.getByRole('combobox', { name: '文本编码' }), 'big5')

    expect(await screen.findByText('測試內容')).not.toBeNull()
    expect(screen.getByText(/仅显示前 5 MiB/)).not.toBeNull()
    expect(read).toHaveBeenLastCalledWith(video.sessionId, 'big5')
    read.mockRestore()
  })

  it('revokes image Blob URLs when the preview is removed', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(new Uint8Array([1, 2, 3]), {
        status: 206,
        headers: { 'Content-Range': 'bytes 0-2/3' },
      }),
    )
    const createObjectURL = vi.fn(() => 'blob:preview')
    const revokeObjectURL = vi.fn()
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: createObjectURL })
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revokeObjectURL })
    const imageInfo: FilePreviewInfo = {
      ...video,
      fileSize: 3,
      kind: 'image',
      contentType: 'image/png',
      limits: { maxBytes: 100 * 1024 * 1024, textBytes: null, maxPixels: 80_000_000 },
    }
    const rendered = render(<FilePreviewModal previewInfo={imageInfo} fileName="photo.png" onClose={vi.fn()} onDownload={vi.fn()} />)

    expect(await screen.findByRole('img', { name: 'photo.png' })).not.toBeNull()
    rendered.unmount()

    expect(createObjectURL).toHaveBeenCalledOnce()
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:preview')
    fetchMock.mockRestore()
  })

  it('pages and zooms a PDF through the range-backed document', async () => {
    const user = userEvent.setup()
    const getPage = vi.fn().mockResolvedValue({
      getViewport: ({ scale }: { scale: number }) => ({ width: 600 * scale, height: 800 * scale }),
      render: () => ({ promise: Promise.resolve(), cancel: vi.fn() }),
    })
    vi.mocked(getDocument).mockReturnValue({
      promise: Promise.resolve({ numPages: 3, getPage }),
      destroy: vi.fn().mockResolvedValue(undefined),
    } as never)
    const pdfInfo: FilePreviewInfo = {
      ...video,
      kind: 'pdf',
      contentType: 'application/pdf',
      decoder: 'pdfjs',
    }
    render(<FilePreviewModal previewInfo={pdfInfo} fileName="report.pdf" onClose={vi.fn()} onDownload={vi.fn()} />)

    await user.click(await screen.findByRole('button', { name: '下一页' }))
    await user.click(screen.getByRole('button', { name: '放大' }))

    expect(await screen.findByLabelText('PDF 第 2 页')).not.toBeNull()
    expect(getPage).toHaveBeenCalledWith(2)
    expect(screen.getByText('125%')).not.toBeNull()
    expect(getDocument).toHaveBeenCalledWith(expect.objectContaining({
      rangeChunkSize: 2 * 1024 * 1024,
      cMapUrl: expect.stringMatching(/\/pdfjs\/cmaps\/$/),
      cMapPacked: true,
      standardFontDataUrl: expect.stringMatching(/\/pdfjs\/standard_fonts\/$/),
      wasmUrl: expect.stringMatching(/\/pdfjs\/wasm\/$/),
      isImageDecoderSupported: false,
      useWorkerFetch: false,
      useWasm: false,
    }))
    vi.restoreAllMocks()
  })

  it('ignores PDF task cancellation during the StrictMode mount probe', async () => {
    const getPage = vi.fn().mockResolvedValue({
      getViewport: () => ({ width: 600, height: 800 }),
      render: () => ({ promise: Promise.resolve(), cancel: vi.fn() }),
    })
    let rejectProbe: (reason: Error) => void = () => {}
    const probePromise = new Promise<never>((_, reject) => {
      rejectProbe = reject
    })
    vi.mocked(getDocument)
      .mockReturnValueOnce({
        promise: probePromise,
        destroy: vi.fn(() => {
          rejectProbe(new Error('Loading aborted'))
          return Promise.resolve()
        }),
      } as never)
      .mockReturnValueOnce({
        promise: Promise.resolve({ numPages: 1, getPage }),
        destroy: vi.fn().mockResolvedValue(undefined),
      } as never)
    const pdfInfo: FilePreviewInfo = {
      ...video,
      kind: 'pdf',
      contentType: 'application/pdf',
      decoder: 'pdfjs',
    }

    render(
      <StrictMode>
        <FilePreviewModal
          previewInfo={pdfInfo}
          fileName="report.pdf"
          onClose={vi.fn()}
          onDownload={vi.fn()}
        />
      </StrictMode>,
    )

    expect(await screen.findByLabelText('PDF 第 1 页')).not.toBeNull()
    expect(screen.queryByText(/无法解析此 PDF/)).toBeNull()
    expect(getDocument).toHaveBeenCalledTimes(2)
    vi.restoreAllMocks()
  })

  it('keeps the PDF loading task alive when the modal parent rerenders', async () => {
    const getPage = vi.fn().mockResolvedValue({
      getViewport: () => ({ width: 600, height: 800 }),
      render: () => ({ promise: Promise.resolve(), cancel: vi.fn() }),
    })
    const destroy = vi.fn().mockResolvedValue(undefined)
    vi.mocked(getDocument).mockReturnValue({
      promise: Promise.resolve({ numPages: 1, getPage }),
      destroy,
    } as never)
    const pdfInfo: FilePreviewInfo = {
      ...video,
      kind: 'pdf',
      contentType: 'application/pdf',
      decoder: 'pdfjs',
    }
    const rendered = render(
      <FilePreviewModal
        previewInfo={pdfInfo}
        fileName="report.pdf"
        onClose={vi.fn()}
        onDownload={vi.fn()}
      />,
    )
    expect(await screen.findByLabelText('PDF 第 1 页')).not.toBeNull()

    rendered.rerender(
      <FilePreviewModal
        previewInfo={pdfInfo}
        fileName="report.pdf"
        onClose={vi.fn()}
        onDownload={vi.fn()}
      />,
    )

    expect(getDocument).toHaveBeenCalledOnce()
    expect(destroy).not.toHaveBeenCalled()
    vi.restoreAllMocks()
  })

  it('removes active content and external resources from SVG', () => {
    const safe = sanitizeSvgForPreview(`
      <svg xmlns="http://www.w3.org/2000/svg">
        <script>alert(1)</script>
        <foreignObject><iframe src="https://evil.test"></iframe></foreignObject>
        <image href="https://evil.test/pixel.png" onload="alert(2)"></image>
        <rect fill="url(https://evil.test/pattern.svg#x)"></rect>
        <defs><linearGradient id="local"></linearGradient></defs>
        <rect fill="url(#local)"></rect>
      </svg>
    `)

    expect(safe).not.toContain('script')
    expect(safe).not.toContain('foreignObject')
    expect(safe).not.toContain('evil.test')
    expect(safe).toContain('url(#local)')
  })
})
