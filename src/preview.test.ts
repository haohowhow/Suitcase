import { describe, expect, it } from 'vitest'
import { isPreviewableFile } from './preview'

describe('isPreviewableFile', () => {
  it('recognizes local preview families and rejects folders and Office files', () => {
    expect(isPreviewableFile({ name: 'notes.big5.txt', contentType: '', entryType: 'file' })).toBe(true)
    expect(isPreviewableFile({ name: 'camera.CR3', contentType: '', entryType: 'file' })).toBe(true)
    expect(isPreviewableFile({ name: 'vector.svg', contentType: 'image/svg+xml', entryType: 'file' })).toBe(true)
    expect(isPreviewableFile({ name: 'report.docx', contentType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', entryType: 'file' })).toBe(false)
    expect(isPreviewableFile({ name: 'design.psd', contentType: 'image/vnd.adobe.photoshop', entryType: 'file' })).toBe(false)
    expect(isPreviewableFile({ name: 'novel.epub', contentType: 'application/epub+zip', entryType: 'file' })).toBe(false)
    expect(isPreviewableFile({ name: 'photo.jpg', contentType: 'image/jpeg', entryType: 'dir' })).toBe(false)
  })
})
