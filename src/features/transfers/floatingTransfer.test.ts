import { describe, expect, it } from 'vitest'

import type { TransferItem } from '../../types/files'
import {
  shouldPromptForFloatingWindow,
  summarizeFloatingTransfers,
} from './floatingTransfer'

function transfer(overrides: Partial<TransferItem> = {}): TransferItem {
  return {
    id: 'transfer-1',
    name: 'file.bin',
    direction: 'download',
    state: 'running',
    bytesTransferred: 25,
    totalBytes: 100,
    localKind: 'file',
    recoveryKind: 'byte_resume',
    remotePath: null,
    localPath: null,
    startedAt: 1,
    finishedAt: null,
    speedSamples: [],
    ...overrides,
  }
}

describe('summarizeFloatingTransfers', () => {
  it('weights concurrent progress by bytes and reports mixed directions', () => {
    const summary = summarizeFloatingTransfers([
      transfer(),
      transfer({
        id: 'upload',
        direction: 'upload',
        bytesTransferred: 900,
        totalBytes: 900,
      }),
    ])

    expect(summary.mode).toBe('active')
    expect(summary.direction).toBe('mixed')
    expect(summary.taskCount).toBe(2)
    expect(summary.percent).toBe(92.5)
  })

  it('does not invent a percentage when any unfinished total is unknown', () => {
    const summary = summarizeFloatingTransfers([
      transfer(),
      transfer({ id: 'folder', totalBytes: null }),
    ])

    expect(summary.percent).toBeNull()
  })

  it('uses paused only when every unfinished task is paused', () => {
    expect(summarizeFloatingTransfers([
      transfer({ state: 'paused' }),
    ])).toMatchObject({
      mode: 'paused',
      direction: 'download',
      percent: 25,
      taskCount: 1,
    })

    expect(summarizeFloatingTransfers([
      transfer({ state: 'paused' }),
      transfer({ id: 'retrying', state: 'retrying' }),
    ]).mode).toBe('active')
  })

  it('ignores terminal history when reporting idle state', () => {
    expect(summarizeFloatingTransfers([
      transfer({ state: 'completed' }),
      transfer({ id: 'failed', state: 'failed' }),
    ])).toEqual({
      mode: 'idle',
      direction: null,
      percent: null,
      taskCount: 0,
      activityKind: 'none',
      syncPairCount: 0,
      syncIssueCount: 0,
      uploadBytesPerSecond: 0,
      downloadBytesPerSecond: 0,
    })
  })
})

describe('shouldPromptForFloatingWindow', () => {
  it('prompts once only after sign-in and settings load', () => {
    expect(shouldPromptForFloatingWindow(true, true, false, false)).toBe(true)
    expect(shouldPromptForFloatingWindow(false, true, false, false)).toBe(false)
    expect(shouldPromptForFloatingWindow(true, false, false, false)).toBe(false)
    expect(shouldPromptForFloatingWindow(true, true, true, false)).toBe(false)
    expect(shouldPromptForFloatingWindow(true, true, false, true)).toBe(false)
  })
})
