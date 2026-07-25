import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { StrictMode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { koofr } from '../../services/koofr'
import type { VideoStreamInfo } from '../../types/backend'
import { MediaPreviewModal } from './MediaPreviewModal'

const videoStream: VideoStreamInfo = {
  sessionId: 'test-session-id',
  fileSize: 10_000_000,
  contentType: 'video/mp4',
  mediaType: 'video',
}

const audioStream: VideoStreamInfo = {
  sessionId: 'test-session-audio',
  fileSize: 5_000_000,
  contentType: 'audio/mpeg',
  mediaType: 'audio',
}

describe('MediaPreviewModal', () => {
  it('renders a video element for video media type', () => {
    render(
      <MediaPreviewModal
        streamInfo={videoStream}
        fileName="clip.mp4"
        onClose={vi.fn()}
      />,
    )
    const video = document.querySelector('video')
    expect(video).not.toBeNull()
    expect(video?.getAttribute('src')).toContain('test-session-id')
  })

  it('does not close the stream during the StrictMode mount probe', () => {
    const closeVideoStream = vi
      .spyOn(koofr, 'closeVideoStream')
      .mockResolvedValue(undefined)

    render(
      <StrictMode>
        <MediaPreviewModal
          streamInfo={videoStream}
          fileName="clip.mp4"
          onClose={vi.fn()}
        />
      </StrictMode>,
    )

    expect(closeVideoStream).not.toHaveBeenCalled()
    closeVideoStream.mockRestore()
  })

  it('renders an audio element for audio media type', () => {
    render(
      <MediaPreviewModal
        streamInfo={audioStream}
        fileName="track.mp3"
        onClose={vi.fn()}
      />,
    )
    const audio = document.querySelector('audio')
    expect(audio).not.toBeNull()
    expect(audio?.getAttribute('src')).toContain('test-session-audio')
  })

  it('displays the file name in the header', () => {
    render(
      <MediaPreviewModal
        streamInfo={videoStream}
        fileName="my vacation.mp4"
        onClose={vi.fn()}
      />,
    )
    expect(screen.getByText('my vacation.mp4')).not.toBeNull()
  })

  it('calls onClose when the close button is clicked', async () => {
    const user = userEvent.setup()
    const onClose = vi.fn()
    render(
      <MediaPreviewModal
        streamInfo={videoStream}
        fileName="clip.mp4"
        onClose={onClose}
      />,
    )
    await user.click(screen.getByRole('button', { name: '关闭' }))
    expect(onClose).toHaveBeenCalledOnce()
  })

  it('calls onClose when the overlay backdrop is clicked', async () => {
    const user = userEvent.setup()
    const onClose = vi.fn()
    const { container } = render(
      <MediaPreviewModal
        streamInfo={videoStream}
        fileName="clip.mp4"
        onClose={onClose}
      />,
    )
    const overlay = container.querySelector('.media-preview-overlay')
    expect(overlay).not.toBeNull()
    if (overlay) await user.click(overlay)
    expect(onClose).toHaveBeenCalledOnce()
  })

  it('shows an error message when the media fails to load', () => {
    const { container } = render(
      <MediaPreviewModal
        streamInfo={videoStream}
        fileName="bad.mkv"
        onClose={vi.fn()}
      />,
    )
    const video = container.querySelector('video')
    expect(video).not.toBeNull()
    if (video) {
      fireEvent.error(video)
    }
    expect(screen.getByText(/无法播放此格式/)).not.toBeNull()
  })
})
