import { afterEach, describe, expect, it } from 'vitest'
import { installRuntimeTranslations } from './i18n'

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
