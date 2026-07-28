import DOMPurify from 'dompurify'
import {
  AlertCircle,
  ArrowDownToLine,
  ChevronLeft,
  ChevronRight,
  RotateCw,
  Scan,
  WrapText,
  X,
  ZoomIn,
  ZoomOut,
} from 'lucide-react'
import {
  GlobalWorkerOptions,
  PDFDataRangeTransport,
  getDocument,
  type PDFDocumentProxy,
  type RenderTask,
} from 'pdfjs-dist'
import { useCallback, useEffect, useRef, useState } from 'react'
import { koofr } from '../../services/koofr'
import type { FilePreviewInfo, PreviewTextResult } from '../../types/backend'
import { formatBytes } from './filePresentation'

GlobalWorkerOptions.workerSrc = new URL(
  'pdfjs-dist/build/pdf.worker.min.mjs',
  import.meta.url,
).toString()

const PDF_CMAP_URL = new URL('/pdfjs/cmaps/', document.baseURI).toString()
const PDF_STANDARD_FONT_URL = new URL('/pdfjs/standard_fonts/', document.baseURI).toString()
const PDF_WASM_URL = new URL('/pdfjs/wasm/', document.baseURI).toString()

interface FilePreviewModalProps {
  readonly previewInfo: FilePreviewInfo
  readonly fileName: string
  readonly onClose: () => void
  readonly onDownload: () => void
}

const RANGE_SIZE = 2 * 1024 * 1024
const TEXT_ENCODINGS = ['auto', 'utf-8', 'utf-16le', 'utf-16be', 'gb18030', 'big5'] as const

async function fetchRange(
  url: string,
  begin: number,
  endExclusive: number,
  signal: AbortSignal,
) {
  const response = await fetch(url, {
    headers: { Range: `bytes=${begin}-${endExclusive - 1}` },
    signal,
  })
  if (response.status !== 206) throw new Error('preview range request failed')
  const expectedLength = endExclusive - begin
  const contentRange = response.headers.get('Content-Range')
  const expectedRange = `bytes ${begin}-${endExclusive - 1}/`
  if (!contentRange?.startsWith(expectedRange)) throw new Error('preview range response mismatch')
  const bytes = new Uint8Array(await response.arrayBuffer())
  if (bytes.byteLength !== expectedLength) throw new Error('preview range response was truncated')
  return bytes
}

async function fetchAll(info: FilePreviewInfo, signal: AbortSignal) {
  const url = koofr.filePreviewUrl(info.sessionId)
  const output = new Uint8Array(info.fileSize)
  for (let offset = 0; offset < info.fileSize; offset += RANGE_SIZE) {
    const end = Math.min(offset + RANGE_SIZE, info.fileSize)
    output.set(await fetchRange(url, offset, end, signal), offset)
  }
  return output
}

export function sanitizeSvgForPreview(source: string) {
  if (/<!doctype|<!entity/i.test(source)) {
    throw new Error('SVG document declarations are not allowed')
  }
  const sanitized = DOMPurify.sanitize(source, {
    USE_PROFILES: { svg: true, svgFilters: false },
    FORBID_TAGS: ['script', 'style', 'foreignObject', 'iframe', 'object', 'embed', 'audio', 'video'],
    FORBID_ATTR: ['style'],
  })
  const document = new DOMParser().parseFromString(sanitized, 'image/svg+xml')
  if (document.documentElement.localName !== 'svg' || document.querySelector('parsererror')) {
    throw new Error('invalid SVG')
  }
  document.querySelectorAll('*').forEach((node) => {
    for (const attribute of [...node.attributes]) {
      const name = attribute.name.toLowerCase()
      const value = attribute.value.trim()
      if (name.startsWith('on')) {
        node.removeAttribute(attribute.name)
      } else if (['href', 'xlink:href', 'src'].includes(name) && !value.startsWith('#')) {
        node.removeAttribute(attribute.name)
      } else if (/url\s*\(/i.test(value) && !/^url\s*\(\s*#[^)]+\s*\)$/i.test(value)) {
        node.removeAttribute(attribute.name)
      }
    }
  })
  return new XMLSerializer().serializeToString(document)
}

class PreviewPdfTransport extends PDFDataRangeTransport {
  readonly #url: string
  readonly #controller = new AbortController()
  readonly #onFailure: () => void

  constructor(length: number, url: string, onFailure: () => void) {
    super(length, null, false)
    this.#url = url
    this.#onFailure = onFailure
  }

  requestDataRange(begin: number, end: number) {
    void fetchRange(this.#url, begin, end, this.#controller.signal)
      .then((data) => this.onDataRange(begin, data))
      .catch(() => {
        if (!this.#controller.signal.aborted) {
          this.#onFailure()
          this.abort()
        }
      })
  }

  abort() {
    this.#controller.abort()
  }
}

export function FilePreviewModal({
  previewInfo,
  fileName,
  onClose,
  onDownload,
}: FilePreviewModalProps) {
  const [error, setError] = useState('')

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('keydown', handleKeyDown)
      // App owns the session. StrictMode's mount probe must not close it.
    }
  }, [onClose])

  return (
    <div className="file-preview-overlay" onClick={onClose}>
      <div className="file-preview-container" onClick={(event) => event.stopPropagation()}>
        <header className="file-preview-header">
          <div>
            <strong title={fileName}>{fileName}</strong>
            <span>{formatBytes(previewInfo.fileSize)}</span>
          </div>
          <div className="file-preview-header-actions">
            <button type="button" className="secondary-button" onClick={onDownload}>
              <ArrowDownToLine size={16} />下载
            </button>
            <button className="icon-button file-preview-close" type="button" aria-label="关闭" onClick={onClose}>
              <X size={20} />
            </button>
          </div>
        </header>
        <div className="file-preview-body">
          {error ? (
            <div className="file-preview-error">
              <AlertCircle size={48} />
              <p>{error}</p>
              <button type="button" className="primary-button" onClick={onDownload}>下载后打开</button>
            </div>
          ) : previewInfo.kind === 'audio' ? (
            <audio
              src={koofr.filePreviewUrl(previewInfo.sessionId)}
              controls
              autoPlay
              className="file-preview-audio"
              onError={() => setError('无法播放此格式，请下载后使用系统播放器打开。')}
            />
          ) : previewInfo.kind === 'video' ? (
            <video
              src={koofr.filePreviewUrl(previewInfo.sessionId)}
              controls
              autoPlay
              className="file-preview-video"
              onError={() => setError('无法播放此格式，请下载后使用系统播放器打开。')}
            />
          ) : previewInfo.kind === 'pdf' ? (
            <PdfPreview info={previewInfo} onError={() => setError('无法解析此 PDF，请下载后打开。')} />
          ) : previewInfo.kind === 'text' ? (
            <TextPreview info={previewInfo} onError={() => setError('无法读取此文本文件，请下载后打开。')} />
          ) : (
            <ImagePreview info={previewInfo} fileName={fileName} onError={() => setError('无法解析此图片，请下载后打开。')} />
          )}
        </div>
      </div>
    </div>
  )
}

function PdfPreview({ info, onError }: { info: FilePreviewInfo; onError: () => void }) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const viewportRef = useRef<HTMLDivElement>(null)
  const onErrorRef = useRef(onError)
  const [document, setDocument] = useState<PDFDocumentProxy | null>(null)
  const [page, setPage] = useState(1)
  const [scale, setScale] = useState(1)
  const [loading, setLoading] = useState(true)
  onErrorRef.current = onError

  useEffect(() => {
    let disposed = false
    const transport = new PreviewPdfTransport(
      info.fileSize,
      koofr.filePreviewUrl(info.sessionId),
      () => {
        if (!disposed) onErrorRef.current()
      },
    )
    const task = getDocument({
      range: transport,
      rangeChunkSize: RANGE_SIZE,
      cMapUrl: PDF_CMAP_URL,
      cMapPacked: true,
      standardFontDataUrl: PDF_STANDARD_FONT_URL,
      wasmUrl: PDF_WASM_URL,
      disableStream: true,
      disableAutoFetch: true,
      maxImageSize: 80_000_000,
      isImageDecoderSupported: false,
      useWorkerFetch: false,
      useWasm: false,
    })
    void task.promise
      .then((pdf) => {
        if (!disposed) setDocument(pdf)
      })
      .catch(() => {
        if (!disposed) onErrorRef.current()
      })
      .finally(() => {
        if (!disposed) setLoading(false)
      })
    return () => {
      disposed = true
      transport.abort()
      void task.destroy()
    }
  }, [info.fileSize, info.sessionId])

  useEffect(() => {
    if (!document || !canvasRef.current) return
    let cancelled = false
    let renderTask: RenderTask | null = null
    const render = async () => {
      const pdfPage = await document.getPage(page)
      if (cancelled || !canvasRef.current) return
      const viewport = pdfPage.getViewport({ scale })
      const canvas = canvasRef.current
      canvas.width = Math.ceil(viewport.width)
      canvas.height = Math.ceil(viewport.height)
      renderTask = pdfPage.render({ canvas, viewport })
      await renderTask.promise
    }
    void render().catch(() => {
      if (!cancelled) onErrorRef.current()
    })
    return () => {
      cancelled = true
      renderTask?.cancel()
    }
  }, [document, page, scale])

  const fitWidth = useCallback(async () => {
    if (!document || !viewportRef.current) return
    const pdfPage = await document.getPage(page)
    const natural = pdfPage.getViewport({ scale: 1 })
    setScale(Math.max(0.25, Math.min(4, (viewportRef.current.clientWidth - 32) / natural.width)))
  }, [document, page])

  return (
    <div className="pdf-preview">
      <div className="file-preview-toolbar">
        <button type="button" aria-label="上一页" disabled={page <= 1} onClick={() => setPage((value) => value - 1)}><ChevronLeft size={18} /></button>
        <label>第 <input aria-label="页码" type="number" min={1} max={document?.numPages ?? 1} value={page} onChange={(event) => setPage(Math.max(1, Math.min(document?.numPages ?? 1, Number(event.target.value) || 1)))} /> / {document?.numPages ?? '—'} 页</label>
        <button type="button" aria-label="下一页" disabled={!document || page >= document.numPages} onClick={() => setPage((value) => value + 1)}><ChevronRight size={18} /></button>
        <i />
        <button type="button" aria-label="缩小" onClick={() => setScale((value) => Math.max(0.25, value - 0.25))}><ZoomOut size={18} /></button>
        <span>{Math.round(scale * 100)}%</span>
        <button type="button" aria-label="放大" onClick={() => setScale((value) => Math.min(4, value + 0.25))}><ZoomIn size={18} /></button>
        <button type="button" onClick={() => void fitWidth()}><Scan size={17} />适应宽度</button>
      </div>
      <div className="pdf-preview-viewport" ref={viewportRef}>
        {loading ? <p className="file-preview-loading">正在加载预览…</p> : null}
        <canvas ref={canvasRef} aria-label={`PDF 第 ${page} 页`} />
      </div>
    </div>
  )
}

function TextPreview({ info, onError }: { info: FilePreviewInfo; onError: () => void }) {
  const [encoding, setEncoding] = useState('auto')
  const [result, setResult] = useState<PreviewTextResult | null>(null)
  const [wrap, setWrap] = useState(true)

  useEffect(() => {
    setResult(null)
    void koofr.readPreviewText(info.sessionId, encoding)
      .then(setResult)
      .catch(onError)
  }, [encoding, info.sessionId, onError])

  return (
    <div className="text-preview">
      <div className="file-preview-toolbar">
        <label>编码
          <select aria-label="文本编码" value={encoding} onChange={(event) => setEncoding(event.target.value)}>
            {TEXT_ENCODINGS.map((value) => <option value={value} key={value}>{value === 'auto' ? '自动检测' : value.toUpperCase()}</option>)}
          </select>
        </label>
        <span>{result?.encoding ?? '—'}</span>
        <i />
        <button type="button" className={wrap ? 'is-active' : ''} onClick={() => setWrap((value) => !value)}>
          <WrapText size={17} />自动换行
        </button>
      </div>
      {result?.truncated ? <p className="file-preview-warning">文件较大，仅显示前 5 MiB。</p> : null}
      {result?.hadDecodeErrors ? <p className="file-preview-warning">检测到无法按当前编码解码的字符。</p> : null}
      {result ? <pre className={wrap ? 'text-preview-content text-preview-content--wrap' : 'text-preview-content'}>{result.text}</pre> : <p className="file-preview-loading">正在加载预览…</p>}
    </div>
  )
}

function ImagePreview({
  info,
  fileName,
  onError,
}: {
  info: FilePreviewInfo
  fileName: string
  onError: () => void
}) {
  const [url, setUrl] = useState('')
  const [scale, setScale] = useState(1)
  const [rotation, setRotation] = useState(0)
  const [fit, setFit] = useState(true)

  useEffect(() => {
    const controller = new AbortController()
    let objectUrl = ''
    const load = async () => {
      const bytes = await fetchAll(info, controller.signal)
      let blob: Blob
      if (info.decoder === 'heic') {
        const { heicTo } = await import('heic-to/csp')
        const converted = await heicTo({ blob: new Blob([bytes]), type: 'image/jpeg', quality: 0.9 })
        blob = converted instanceof Blob ? converted : new Blob([converted as BlobPart], { type: 'image/jpeg' })
      } else if (info.decoder === 'svg') {
        const source = new TextDecoder('utf-8', { fatal: true }).decode(bytes)
        blob = new Blob([sanitizeSvgForPreview(source)], { type: 'image/svg+xml' })
      } else {
        blob = new Blob([bytes], { type: info.contentType })
      }
      objectUrl = URL.createObjectURL(blob)
      setUrl(objectUrl)
    }
    void load().catch((error) => {
      if (!controller.signal.aborted) onError()
      void error
    })
    return () => {
      controller.abort()
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [info, onError])

  return (
    <div className="image-preview">
      <div className="file-preview-toolbar">
        <button type="button" aria-label="缩小" onClick={() => { setFit(false); setScale((value) => Math.max(0.1, value - 0.1)) }}><ZoomOut size={18} /></button>
        <span>{Math.round(scale * 100)}%</span>
        <button type="button" aria-label="放大" onClick={() => { setFit(false); setScale((value) => Math.min(5, value + 0.1)) }}><ZoomIn size={18} /></button>
        <button type="button" className={fit ? 'is-active' : ''} onClick={() => { setFit(true); setScale(1) }}><Scan size={17} />适应窗口</button>
        <button type="button" onClick={() => { setFit(false); setScale(1) }}>实际大小</button>
        <button type="button" onClick={() => setRotation((value) => (value + 90) % 360)}><RotateCw size={17} />旋转</button>
      </div>
      <div className="image-preview-viewport">
        {url ? <img className={fit ? 'image-preview__image--fit' : ''} src={url} alt={fileName} style={{ transform: `scale(${scale}) rotate(${rotation}deg)` }} onError={onError} /> : <p className="file-preview-loading">正在加载预览…</p>}
      </div>
    </div>
  )
}
