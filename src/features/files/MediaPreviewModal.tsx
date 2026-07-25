import { AlertCircle, X } from 'lucide-react'
import { useEffect, useState } from 'react'
import { koofr } from '../../services/koofr'
import type { VideoStreamInfo } from '../../types/backend'

interface MediaPreviewModalProps {
  readonly streamInfo: VideoStreamInfo
  readonly fileName: string
  readonly onClose: () => void
}

export function MediaPreviewModal({ streamInfo, fileName, onClose }: MediaPreviewModalProps) {
  const [error, setError] = useState(false)
  const url = koofr.videoStreamUrl(streamInfo.sessionId)

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('keydown', handleKeyDown)
      // App owns the stream session. Closing it here would delete the session
      // during React StrictMode's development-only effect cleanup probe.
    }
  }, [onClose])

  return (
    <div className="media-preview-overlay" onClick={onClose}>
      <div className="media-preview-container" onClick={(event) => event.stopPropagation()}>
        <div className="media-preview-header">
          <span className="media-preview-title" title={fileName}>{fileName}</span>
          <button className="icon-button media-preview-close" type="button" aria-label="关闭" onClick={onClose}>
            <X size={20} />
          </button>
        </div>
        {error ? (
          <div className="media-preview-error">
            <AlertCircle size={48} />
            <p>无法播放此格式，请下载后使用系统播放器打开。</p>
          </div>
        ) : streamInfo.mediaType === 'audio' ? (
          <audio
            src={url}
            controls
            autoPlay
            className="media-preview-audio"
            onError={() => setError(true)}
          />
        ) : (
          <video
            src={url}
            controls
            autoPlay
            className="media-preview-video"
            onError={() => setError(true)}
          />
        )}
      </div>
    </div>
  )
}
