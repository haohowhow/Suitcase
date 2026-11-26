import { Clipboard, Copy, Download, LoaderCircle, RefreshCw, Search, Trash2, Upload } from 'lucide-react'
import { useCallback, useDeferredValue, useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import { commandErrorMessage } from '../../services/koofr'
import { publicLinks } from '../../services/publicLinks'
import type { KoofrMount, PublicLink } from '../../types/backend'

interface ShareLinksWorkspaceProps {
  readonly mounts: readonly KoofrMount[]
  readonly headerTabs?: ReactNode
}

function initialMountId(mounts: readonly KoofrMount[]) {
  return mounts.find((mount) => mount.isPrimary)?.id ?? mounts[0]?.id ?? ''
}

export function ShareLinksWorkspace({ mounts, headerTabs }: ShareLinksWorkspaceProps) {
  const [mountId, setMountId] = useState(() => initialMountId(mounts))
  const [links, setLinks] = useState<PublicLink[]>([])
  const [query, setQuery] = useState('')
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [confirmingId, setConfirmingId] = useState('')
  const [copiedId, setCopiedId] = useState('')
  const deferredQuery = useDeferredValue(query)

  useEffect(() => {
    if (!mountId || !mounts.some((mount) => mount.id === mountId)) {
      const next = initialMountId(mounts)
      if (next) setMountId(next)
    }
  }, [mountId, mounts])

  const filteredLinks = useMemo(() => {
    const normalized = deferredQuery.trim().toLocaleLowerCase('zh-CN')
    if (!normalized) return links
    return links.filter((link) => (
      link.name.toLocaleLowerCase('zh-CN').includes(normalized)
      || link.path.toLocaleLowerCase('zh-CN').includes(normalized)
      || (link.shortUrl || link.url).toLocaleLowerCase('zh-CN').includes(normalized)
    ))
  }, [deferredQuery, links])

  const groupedLinks = useMemo(() => ({
    download: filteredLinks.filter((link) => link.kind === 'download'),
    upload: filteredLinks.filter((link) => link.kind === 'upload'),
  }), [filteredLinks])

  const loadLinks = useCallback(async () => {
    if (!mountId) return
    setLoading(true)
    setError('')
    setConfirmingId('')
    try {
      setLinks(await publicLinks.list(mountId))
    } catch (loadError) {
      setLinks([])
      setError(commandErrorMessage(loadError, '无法读取分享链接。'))
    } finally {
      setLoading(false)
    }
  }, [mountId])

  useEffect(() => {
    void loadLinks()
  }, [loadLinks])

  const removeLink = async (link: PublicLink) => {
    if (confirmingId !== link.id) {
      setConfirmingId(link.id)
      return
    }
    setBusy(true)
    setError('')
    try {
      await publicLinks.remove(mountId, link.id, link.kind)
      setLinks((current) => current.filter((candidate) => candidate.id !== link.id))
      setConfirmingId('')
    } catch (removeError) {
      setError(commandErrorMessage(removeError, '无法撤销分享链接。'))
    } finally {
      setBusy(false)
    }
  }

  const copyLink = async (link: PublicLink) => {
    try {
      await navigator.clipboard.writeText(link.shortUrl || link.url)
      setCopiedId(link.id)
    } catch {
      setError('无法自动复制，请选中地址后手动复制。')
    }
  }

  return (
    <main className="workspace links-workspace" aria-label="分享链接">
      <div className="workspace__topbar links-workspace__topbar">
        {headerTabs ?? null}
        <label className="search-box collection-search">
          <Search size={15} strokeWidth={1.8} />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="筛选分享链接"
            aria-label="筛选分享链接"
          />
        </label>
        <div className="top-actions">
          <select
            className="mount-select"
            value={mountId}
            aria-label="存储位置"
            disabled={busy || mounts.length === 0}
            onChange={(event) => setMountId(event.target.value)}
          >
            {mounts.map((mount) => (
              <option translate="no" key={mount.id} value={mount.id}>{mount.name}</option>
            ))}
          </select>
          <button
            className="icon-button icon-button--bordered"
            type="button"
            aria-label="刷新分享链接"
            title="刷新"
            disabled={loading || busy || !mountId}
            onClick={() => void loadLinks()}
          >
            <RefreshCw className={loading ? 'icon-spin' : ''} size={15} strokeWidth={1.8} />
          </button>
        </div>
      </div>

      <div className="workspace__heading-row">
        <header className="workspace__heading">
          <h1>分享链接</h1>
          <p>
            管理已创建的公开下载链接和接收文件链接 · {links.length} 个链接
          </p>
        </header>
      </div>

      <div className="links-workspace__body">
        <p className="links-workspace__tip">
          创建新链接时，请先在“我的文件”中选中文件或文件夹，再点击“分享”。
        </p>

        {error ? <p className="settings-error" role="alert">{error}</p> : null}

        {loading ? (
          <div className="share-links-loading" role="status">
            <LoaderCircle className="spin" />
            正在读取分享链接…
          </div>
        ) : (
          <div className="share-link-groups">
            <LinkGroup
              title="下载链接"
              emptyText={query ? '没有匹配的下载链接' : '还没有下载链接'}
              icon={<Download />}
              links={groupedLinks.download}
              busy={busy}
              confirmingId={confirmingId}
              copiedId={copiedId}
              onCopy={copyLink}
              onRemove={removeLink}
            />
            <LinkGroup
              title="接收文件链接"
              emptyText={query ? '没有匹配的接收文件链接' : '还没有接收文件链接'}
              icon={<Upload />}
              links={groupedLinks.upload}
              busy={busy}
              confirmingId={confirmingId}
              copiedId={copiedId}
              onCopy={copyLink}
              onRemove={removeLink}
            />
          </div>
        )}
      </div>
    </main>
  )
}

interface LinkGroupProps {
  readonly title: string
  readonly emptyText: string
  readonly icon: ReactNode
  readonly links: readonly PublicLink[]
  readonly busy: boolean
  readonly confirmingId: string
  readonly copiedId: string
  readonly onCopy: (link: PublicLink) => Promise<void>
  readonly onRemove: (link: PublicLink) => Promise<void>
}

function LinkGroup({
  title,
  emptyText,
  icon,
  links,
  busy,
  confirmingId,
  copiedId,
  onCopy,
  onRemove,
}: LinkGroupProps) {
  return (
    <section className="share-link-group">
      <h3>
        {icon}
        {title}
        <span>{links.length}</span>
      </h3>
      {links.length === 0 ? (
        <p className="share-link-empty">
          <Clipboard aria-hidden="true" />
          {emptyText}
        </p>
      ) : (
        <div className="share-link-group__list">
          {links.map((link) => (
            <article className="share-link-card" key={`${link.kind}-${link.id}`}>
              <span className="share-link-card__details">
                <strong translate="no">{link.name || link.path}</strong>
                <small>
                  {link.path} · 已访问 {link.counter} 次{link.hasPassword ? ' · 受密码保护' : ''}
                </small>
                <input
                  aria-label={`${link.name || link.path} 的链接地址`}
                  value={link.shortUrl || link.url}
                  readOnly
                  onFocus={(event) => event.currentTarget.select()}
                />
              </span>
              <span className="share-link-card__actions">
                <button
                  className="secondary-button"
                  type="button"
                  disabled={busy}
                  onClick={() => void onCopy(link)}
                >
                  <Copy aria-hidden="true" />
                  {copiedId === link.id ? '已复制' : '复制'}
                </button>
                <button
                  className="danger-button"
                  type="button"
                  aria-label={`${confirmingId === link.id ? '确认撤销' : '撤销'} ${link.name || link.path}`}
                  disabled={busy}
                  onClick={() => void onRemove(link)}
                >
                  <Trash2 aria-hidden="true" />
                  {confirmingId === link.id ? '再次点击确认' : '撤销'}
                </button>
              </span>
            </article>
          ))}
        </div>
      )}
    </section>
  )
}
