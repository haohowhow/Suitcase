// @vitest-environment node
import { describe, expect, it, vi } from 'vitest'
import { forgejoApi, releaseNotes, uploadDraft } from './release-forgejo.mjs'

describe('Forgejo release preparation', () => {
  it('extracts only the selected version, including the final changelog section', () => {
    const changelog = '## [Unreleased]\r\nFuture\r\n## [2.6.0] - 2026-08-21\r\nCurrent\r\n## [2.5.0]\r\nOlder'
    expect(releaseNotes(changelog, '2.6.0')).toContain('Current')
    expect(releaseNotes(changelog, '2.6.0')).not.toContain('Older')
    expect(releaseNotes(changelog, '2.5.0')).toContain('Older')
    expect(() => releaseNotes(changelog, '1.0.0')).toThrow('missing')
  })

  it('creates only a draft after confirming the remote tag commit', async () => {
    const api = vi.fn()
      .mockResolvedValueOnce({ commit: { sha: 'commit-a' } })
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce({ id: 7, draft: true })
      .mockResolvedValueOnce({ id: 8 })
    await uploadDraft({ tag: 'v2.6.0', commit: 'commit-a', body: 'Notes', assets: [
      { name: 'SHA256SUMS', blob: new Blob(['synthetic-checksum']) },
    ] }, api)
    expect(api.mock.calls[2]).toEqual(['POST', '/releases', expect.objectContaining({ draft: true })])
    expect(api.mock.calls[3][1]).toBe('/releases/7/assets?name=SHA256SUMS')
    expect(api.mock.calls.some(([method]) => method === 'PATCH')).toBe(false)
  })

  it.each([null, { commit: { sha: 'another-commit' } }])('refuses a missing or different remote tag', async (remoteTag) => {
    const api = vi.fn().mockResolvedValueOnce(remoteTag)
    await expect(uploadDraft({ tag: 'v2.6.0', commit: 'commit-a', assets: [] }, api)).rejects.toThrow('must match')
    expect(api).toHaveBeenCalledTimes(1)
  })

  it.each([{ id: 7, draft: false }, { id: 7, draft: true, assets: [{ id: 8 }] }])(
    'refuses to overwrite published releases or existing attachments', async (release) => {
      const api = vi.fn().mockResolvedValueOnce({ commit: { sha: 'commit-a' } }).mockResolvedValueOnce(release)
      await expect(uploadDraft({ tag: 'v2.6.0', commit: 'commit-a', assets: [] }, api)).rejects.toThrow('overwrite')
      expect(api).toHaveBeenCalledTimes(2)
    },
  )

  it('rejects redirects and never exposes response bodies or credentials in errors', async () => {
    const fetchImpl = vi.fn().mockResolvedValue({ status: 403, ok: false })
    const api = forgejoApi('synthetic-test-token', fetchImpl)
    await expect(api('GET', '/tags/v2.6.0')).rejects.toThrow('HTTP 403')
    expect(fetchImpl).toHaveBeenCalledWith(expect.stringContaining('/api/v1/repos/wenhaozheng/Suitcase/'),
      expect.objectContaining({ redirect: 'error' }))
    fetchImpl.mockRejectedValueOnce(new Error('synthetic-test-token'))
    await expect(api('GET', '/tags/v2.6.0')).rejects.not.toThrow('synthetic-test-token')
    fetchImpl.mockResolvedValueOnce({ status: 200, ok: true, json: () => {
      throw new Error('private-response-body')
    } })
    await expect(api('GET', '/tags/v2.6.0')).rejects.toThrow('invalid JSON response')
  })
})
