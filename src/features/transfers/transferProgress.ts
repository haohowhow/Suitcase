import type { TransferProgress } from '../../types/backend'
import type { TransferItem } from '../../types/files'

const MAX_SPEED_SAMPLES = 300
const MIN_SPEED_SAMPLE_INTERVAL_MS = 1000

export function countActiveDownloads(items: readonly TransferItem[]) {
  return items.filter((transfer) => (
    transfer.direction === 'download'
    && (transfer.state === 'queued' || transfer.state === 'running' || transfer.state === 'retrying')
  )).length
}

export function mergeTransferProgress(
  items: readonly TransferItem[],
  progress: TransferProgress,
): TransferItem[] {
  const recordedAt = Date.now()
  return items.map((transfer) => transfer.id === progress.transferId
    ? (() => {
        const previousSample = transfer.speedSamples.at(-1)
        const terminal = progress.state === 'completed'
          || progress.state === 'cancelled'
          || progress.state === 'failed'
        const active = progress.state === 'queued'
          || progress.state === 'running'
          || progress.state === 'retrying'
        const restartedUpload = transfer.direction === 'upload'
          && transfer.recoveryKind === 'restart'
          && progress.state === 'running'
          && transfer.state !== 'running'
        const sample = { recordedAt, bytesTransferred: progress.bytesTransferred }
        const shouldSample = sample.bytesTransferred !== (previousSample?.bytesTransferred ?? transfer.bytesTransferred)
          && (
            previousSample === undefined
            || recordedAt - previousSample.recordedAt >= MIN_SPEED_SAMPLE_INTERVAL_MS
            || terminal
          )
        let speedSamples = transfer.speedSamples
        // Whole-file uploads restart their byte counter after a retry. Start
        // sampling at the new attempt so the first rate uses its own baseline.
        if (restartedUpload) {
          speedSamples = [sample]
        } else if (shouldSample) {
          speedSamples = [...speedSamples, sample].slice(-MAX_SPEED_SAMPLES)
        }
        return {
          ...transfer,
          state: progress.state,
          bytesTransferred: progress.bytesTransferred,
          totalBytes: progress.totalBytes ?? transfer.totalBytes,
          splitProgress: progress.splitProgress ?? transfer.splitProgress,
          finishedAt: active ? null : terminal ? transfer.finishedAt ?? recordedAt : transfer.finishedAt,
          speedSamples,
        }
      })()
    : transfer)
}
