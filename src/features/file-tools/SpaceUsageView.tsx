import {
  ArrowLeft,
  ChevronRight,
  FolderOpen,
  HardDrive,
  LoaderCircle,
  Search,
  TriangleAlert,
} from 'lucide-react'
import { useDeferredValue, useMemo, useState } from 'react'
import { FileTypeIcon } from '../../components/FileTypeIcon'
import type { FileToolsEntry, FileToolsResults } from '../../types/fileTools'
import { fileKindByName, formatBytes } from '../../utils/format'
import {
  entryBasename,
  entryKey,
  formatSharePercent,
} from './fileToolsHelpers'

const PAGE_SIZE = 60

interface SpaceUsageViewProps {
  readonly results: FileToolsResults
  readonly breadcrumb: readonly FileToolsEntry[]
  readonly childrenEntries: readonly FileToolsEntry[]
  readonly busy: boolean
  readonly browsing: boolean
  readonly onBrowse: (next: readonly FileToolsEntry[]) => void
  readonly canOpenInFiles?: (mountId: string) => boolean
  readonly onOpenInFiles?: (mountId: string, path: string, directory: boolean) => void
}

export function SpaceUsageView({
  results,
  breadcrumb,
  childrenEntries,
  busy,
  browsing,
  onBrowse,
  canOpenInFiles,
  onOpenInFiles,
}: SpaceUsageViewProps) {
  const [query, setQuery] = useState('')
  const [pagination, setPagination] = useState<{ scope: string; limit: number }>({
    scope: 'roots',
    limit: PAGE_SIZE,
  })
  const deferredQuery = useDeferredValue(query)

  const currentFolder = breadcrumb.at(-1)
  const scopeKey = currentFolder ? entryKey(currentFolder) : 'roots'
  const rawItems = currentFolder ? childrenEntries : results.roots
  const limit = pagination.scope === scopeKey ? pagination.limit : PAGE_SIZE

  const mountFor = (entry: FileToolsEntry) => results.mounts[entry.id.scope]

  const filteredItems = useMemo(() => {
    const normalized = deferredQuery.trim().toLocaleLowerCase('zh-CN')
    if (!normalized) return rawItems
    return rawItems.filter((entry) => {
      const mountName = results.mounts[entry.id.scope]?.name ?? ''
      const label = currentFolder ? entryBasename(entry.path) : `${mountName} ${entry.path}`
      return label.toLocaleLowerCase('zh-CN').includes(normalized)
        || entry.path.toLocaleLowerCase('zh-CN').includes(normalized)
    })
  }, [currentFolder, deferredQuery, rawItems, results.mounts])

  const referenceTotal = useMemo(() => {
    const siblingSum = rawItems.reduce((sum, entry) => sum + entry.size, 0)
    if (currentFolder) return Math.max(currentFolder.size, siblingSum)
    return siblingSum
  }, [currentFolder, rawItems])

  const totalFilesHere = useMemo(() => {
    if (currentFolder) return currentFolder.files
    return rawItems.reduce((sum, entry) => sum + entry.files, 0)
  }, [currentFolder, rawItems])

  const visibleItems = filteredItems.slice(0, limit)
  const remaining = filteredItems.length - visibleItems.length

  const goToScope = (next: readonly FileToolsEntry[]) => {
    setQuery('')
    onBrowse(next)
  }

  return (
    <section className="ft-panel" aria-label="空间占用">
      <div className="ft-toolbar">
        <div className="ft-toolbar__nav">
          {currentFolder ? (
            <button
              className="icon-button icon-button--bordered"
              type="button"
              aria-label="上一级"
              title="上一级"
              disabled={busy}
              onClick={() => goToScope(breadcrumb.slice(0, -1))}
            >
              <ArrowLeft size={15} strokeWidth={1.85} />
            </button>
          ) : null}

          <nav className="breadcrumb ft-breadcrumb" aria-label="空间占用路径">
            <button
              type="button"
              disabled={busy || breadcrumb.length === 0}
              onClick={() => goToScope([])}
            >
              按存储位置统计
            </button>
            {breadcrumb.map((crumb, index) => {
              const mount = mountFor(crumb)
              const isLast = index === breadcrumb.length - 1
              const label = index === 0
                ? (crumb.path === '/' ? (mount?.name ?? '存储位置') : `${mount?.name ?? ''} · ${entryBasename(crumb.path)}`)
                : entryBasename(crumb.path)
              return (
                <span className="breadcrumb__segment" key={entryKey(crumb)}>
                  <span>/</span>
                  <button
                    type="button"
                    translate="no"
                    title={crumb.path}
                    disabled={busy || isLast}
                    onClick={() => goToScope(breadcrumb.slice(0, index + 1))}
                  >
                    {label}
                  </button>
                </span>
              )
            })}
          </nav>
        </div>

        <div className="ft-toolbar__right">
          <div className="ft-inline-stat">
            <strong>{formatBytes(referenceTotal)}</strong>
            <span>·</span>
            <span>{totalFilesHere} <span>个文件</span></span>
          </div>
          <label className="search-box ft-search">
            <Search size={14} strokeWidth={1.8} />
            <input
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="筛选当前层级"
              aria-label="筛选当前层级"
            />
          </label>
        </div>
      </div>

      {browsing ? (
        <div className="ft-inline-loading" role="status">
          <LoaderCircle size={15} className="spin" aria-hidden="true" />
          <span>正在加载目录占用…</span>
        </div>
      ) : null}

      {filteredItems.length === 0 ? (
        <div className="empty-state ft-empty-compact">
          <span className="ft-empty-icon" aria-hidden="true">
            <HardDrive size={22} strokeWidth={1.7} />
          </span>
          <h2>{query.trim() ? '没有匹配的项目' : '没有已扫描的项目。'}</h2>
          <p>{query.trim() ? '尝试调整筛选关键字，或清空搜索条件。' : '当前目录没有可统计的子项。'}</p>
        </div>
      ) : (
        <div className="ft-table" role="table" aria-label="空间占用明细">
          <div className="ft-table__head ft-usage-grid" role="row">
            <span role="columnheader">名称</span>
            <span role="columnheader">空间占比</span>
            <span className="ft-align-right" role="columnheader">大小</span>
            <span className="ft-align-right" role="columnheader">文件数</span>
            <span className="ft-align-right" role="columnheader">操作</span>
          </div>

          <div className="ft-table__body" role="rowgroup">
            {visibleItems.map((entry) => {
              const mount = mountFor(entry)
              const mountName = mount?.name ?? ''
              const basename = entryBasename(entry.path)
              const isRootRow = !currentFolder
              const displayTitle = isRootRow
                ? (entry.path === '/' ? mountName : basename)
                : basename
              const displaySubtitle = isRootRow
                ? (entry.path === '/' ? (mount?.primary ? '主存储 · 根目录' : '外部或共享 · 根目录') : `${mountName} · ${entry.path}`)
                : null
              const ratio = referenceTotal > 0 ? Math.min(100, (entry.size / referenceTotal) * 100) : 0
              const kind = entry.directory ? 'folder' : fileKindByName(basename)
              const openable = Boolean(mount && onOpenInFiles && canOpenInFiles?.(mount.id))

              return (
                <div
                  className={`ft-table__row ft-usage-grid${entry.canBrowse ? ' ft-table__row--interactive' : ''}`}
                  key={entryKey(entry)}
                  role="row"
                  onDoubleClick={() => {
                    if (entry.canBrowse && !busy) goToScope([...breadcrumb, entry])
                  }}
                >
                  <div className="ft-entry-cell" role="cell">
                    <FileTypeIcon kind={kind} />
                    <div className="ft-entry-cell__text">
                      <div className="ft-entry-cell__title-row">
                        {entry.canBrowse ? (
                          <button
                            className="ft-entry-link"
                            type="button"
                            translate="no"
                            title={isRootRow ? `${mountName} · ${entry.path}` : entry.path}
                            disabled={busy}
                            onClick={() => goToScope([...breadcrumb, entry])}
                          >
                            {displayTitle}
                          </button>
                        ) : (
                          <span
                            className="ft-entry-name"
                            translate="no"
                            title={isRootRow ? `${mountName} · ${entry.path}` : entry.path}
                          >
                            {displayTitle}
                          </span>
                        )}
                        {!entry.complete ? (
                          <span className="ft-pill ft-pill--warning" title="部分子目录无法读取或受保护">
                            <TriangleAlert size={11} aria-hidden="true" />
                            统计不完整
                          </span>
                        ) : null}
                      </div>
                      {displaySubtitle ? (
                        <span className="ft-entry-sub" translate={entry.path === '/' ? undefined : 'no'}>
                          {displaySubtitle}
                        </span>
                      ) : null}
                    </div>
                  </div>

                  <div className="ft-bar-cell" role="cell">
                    <div className="ft-bar-track" aria-hidden="true">
                      <span
                        className={`ft-bar-fill${entry.directory ? '' : ' ft-bar-fill--file'}`}
                        style={{ width: `${ratio > 0 ? Math.max(ratio, 1.5) : 0}%` }}
                      />
                    </div>
                    <span className="ft-bar-pct">{formatSharePercent(entry.size, referenceTotal)}</span>
                  </div>

                  <div className="ft-num-cell ft-align-right" role="cell">
                    {formatBytes(entry.complete || entry.directory ? entry.size : null)}
                  </div>

                  <div className="ft-muted-cell ft-align-right" role="cell">
                    {entry.directory ? (
                      <span>{entry.files} <span>个文件</span></span>
                    ) : (
                      <span>—</span>
                    )}
                  </div>

                  <div className="ft-actions-cell" role="cell">
                    {openable && mount && onOpenInFiles ? (
                      <button
                        className="row-action ft-row-action"
                        type="button"
                        aria-label={`打开 ${displayTitle} 所在位置`}
                        title="在我的文件中定位"
                        disabled={busy}
                        onClick={() => onOpenInFiles(mount.id, entry.path, entry.directory)}
                      >
                        <FolderOpen size={15} strokeWidth={1.8} />
                      </button>
                    ) : null}
                    {entry.canBrowse ? (
                      <button
                        className="row-action ft-row-action"
                        type="button"
                        aria-label="查看目录"
                        title="查看目录"
                        disabled={busy}
                        onClick={() => goToScope([...breadcrumb, entry])}
                      >
                        <ChevronRight size={16} strokeWidth={1.85} />
                      </button>
                    ) : null}
                  </div>
                </div>
              )
            })}
          </div>

          {remaining > 0 ? (
            <div className="ft-load-more">
              <button
                className="secondary-button"
                type="button"
                onClick={() => setPagination({ scope: scopeKey, limit: limit + PAGE_SIZE })}
              >
                显示更多
              </button>
              <span className="ft-load-more__meta">剩余 {remaining} 项</span>
            </div>
          ) : null}
        </div>
      )}
    </section>
  )
}
