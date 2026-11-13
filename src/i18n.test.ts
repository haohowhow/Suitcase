import { afterEach, describe, expect, it } from 'vitest'
import { installRuntimeTranslations, translateUiText } from './i18n'

function mount(html: string) {
  const root = document.createElement('div')
  root.innerHTML = html
  document.body.append(root)
  return root
}

// MutationObserver callbacks are delivered asynchronously.
function nextMutationBatch() {
  return new Promise((resolve) => setTimeout(resolve, 0))
}

describe('installRuntimeTranslations', () => {
  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('translates interface text but leaves marked user content alone in English', () => {
    const root = mount(
      '<button title="设置">设置</button><strong translate="no" title="设置">设置</strong>',
    )

    const stop = installRuntimeTranslations(root, 'enUs')

    expect(root.querySelector('button')?.textContent).toBe('Settings')
    expect(root.querySelector('button')?.getAttribute('title')).toBe('Settings')
    expect(root.querySelector('strong')?.textContent).toBe('设置')
    expect(root.querySelector('strong')?.getAttribute('title')).toBe('设置')
    stop()
  })

  it('does not rename an English-named folder while the interface is Chinese', () => {
    const root = mount(
      '<span>Download</span><div translate="no"><strong>Download</strong></div>',
    )

    const stop = installRuntimeTranslations(root, 'zhCn')

    expect(root.querySelector('span')?.textContent).toBe('下载')
    expect(root.querySelector('strong')?.textContent).toBe('Download')
    stop()
  })

  it('keeps skipping user content that is added or edited later', async () => {
    const root = mount('<ul></ul>')
    const stop = installRuntimeTranslations(root, 'zhCn')

    const item = document.createElement('li')
    item.innerHTML = '<strong translate="no">Logs</strong><span>Logs</span>'
    root.querySelector('ul')?.append(item)
    await nextMutationBatch()

    expect(item.querySelector('strong')?.textContent).toBe('Logs')
    expect(item.querySelector('span')?.textContent).toBe('日志')

    const name = item.querySelector('strong')?.firstChild
    if (name) name.textContent = 'History'
    await nextMutationBatch()

    expect(item.querySelector('strong')?.textContent).toBe('History')
    stop()
  })
})

describe('translateUiText', () => {
  it('translates catalog entries and keeps surrounding whitespace', () => {
    expect(translateUiText(' 个项目 · ', 'en-US')).toBe(' items · ')
    expect(translateUiText('回收站是空的', 'en-US')).toBe('Trash is empty')
  })

  it('translates strings that embed names and numbers', () => {
    expect(translateUiText('已创建文件夹“报告”', 'en-US')).toBe('Folder “报告” created')
    expect(translateUiText('已删除 2 个项目；其余项目删除失败。', 'en-US')).toBe(
      'Deleted 2 items; The remaining items could not be deleted.',
    )
    expect(translateUiText('预览 年报.pdf', 'en-US')).toBe('Preview 年报.pdf')
    expect(translateUiText('打开 年报.pdf 所在文件夹', 'en-US')).toBe('Show the folder containing 年报.pdf')
    expect(translateUiText('筛选最近的文件', 'en-US')).toBe('Filter recent files')
    expect(translateUiText('1 天', 'en-US')).toBe('1 day')
    expect(translateUiText('30 天', 'en-US')).toBe('30 days')
    expect(translateUiText('已使用 12.5% 存储空间', 'en-US')).toBe('12.5% of storage used')
  })

  it('leaves English user text and unknown Chinese text unchanged', () => {
    expect(translateUiText('Quarterly report.pdf', 'en-US')).toBe('Quarterly report.pdf')
    expect(translateUiText('某个没有收录的句子', 'en-US')).toBe('某个没有收录的句子')
  })

  it('translates the backend error messages shown to users', () => {
    expect(translateUiText('无法连接 Koofr，请检查网络后重试。', 'en-US')).toBe(
      'Cannot reach Koofr. Check your network and try again.',
    )
  })
})
