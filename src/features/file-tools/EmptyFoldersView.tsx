import {
  CheckCircle2,
  FolderOpen,
  Lock,
  Search,
  Trash2,
} from 'lucide-react'
import { useDeferredValue, useMemo, useState } from 'react'
import { FileTypeIcon } from '../../components/FileTypeIcon'
import type { FileToolsEntry, FileToolsResults } from '../../types/fileTools'
import {
  entryBasename,
  entryKey,
  entryParentPath,
  type FileToolsRemoval,
} from './fileToolsHelpers'

const PAGE_SIZE = 60
type EmptyFilter = 'all' | 'removable' | 'readonly'

interface EmptyFoldersViewProps {
  readonly results: FileToolsResults
  readonly busy: boolean
  readonly onRemove: (removal: FileToolsRemoval) => void
  readonly canOpenInFiles?: (mountId: string) => boolean
  readonly onOpenInFiles?: (mountId: string, path: string, directory: boolean) => void
}

export function EmptyFoldersView({
  results,
  busy,
  onRemove,
  canOpenInFiles,
  onOpenInFiles,
}: EmptyFoldersViewProps) {
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<EmptyFilter>('all')
  const [limit, setLimit] = useState(PAGE_SIZE)
  const deferredQuery = useDeferredValue(query)

  const removableCount = useMemo(
    () => results.emptyFolders.filter((entry) => entry.removable).length,
    [results.emptyFolders],
  )
  const readonlyCount = results.emptyFolders.length - removableCount

  const effectiveFilter = (filter === 'removable' && removableCount === 0)
    || (filter === 'readonly' && readonlyCount === 0)
    ? 'all'
    : filter

  const filtered = useMemo(() => {
    const normalized = deferredQuery.trim().toLocaleLowerCase('zh-CN')
    return results.emptyFolders.filter((entry) => {
      if (effectiveFilter === 'removable' && !entry.removable) return false
      if (effectiveFilter === 'readonly' && entry.removable) return false
      if (!normalized) return true
      const mountName = results.mounts[entry.id.scope]?.name ?? ''
      return entry.path.toLocaleLowerCase('zh-CN').includes(normalized)
        || mountName.toLocaleLowerCase('zh-CN').includes(normalized)
    })
  }, [deferredQuery, effectiveFilter, results.emptyFolders, results.mounts])

  const visible = filtered.slice(0, limit)
  const remaining = filtered.length - visible.length

  const mountFor = (entry: FileToolsEntry) => results.mounts[entry.id.scope]

  return (
    <section className="ft-panel" aria-label="空文件夹">
      <div className="ft-toolbar">
        <div className="ft-toolbar__left">
          <div className="ft-toolbar__heading">
            <strong>空文件夹</strong>
            <span className="ft-count-pill">{results.emptyFolders.length}</span>
          </div>
          {removableCount > 0 && readonlyCount > 0 ? (
            <div className="ft-filter-chips" role="group" aria-label="筛选空文件夹">
              <button
                className={`sync-filter-chip${effectiveFilter === 'all' ? ' sync-filter-chip--active' : ''}`}
                type="button"
                onClick={() => { setFilter('all'); setLimit(PAGE_SIZE) }}
              >
                全部 <span>{results.emptyFolders.length}</span>
              </button>
              <button
                className={`sync-filter-chip${effectiveFilter === 'removable' ? ' sync-filter-chip--active' : ''}`}
                type="button"
                onClick={() => { setFilter('removable'); setLimit(PAGE_SIZE) }}
              >
                可清理 <span>{removableCount}</span>
              </button>
              <button
                className={`sync-filter-chip${effectiveFilter === 'readonly' ? ' sync-filter-chip--active' : ''}`}
                type="button"
                onClick={() => { setFilter('readonly'); setLimit(PAGE_SIZE) }}
              >
                仅供查看 <span>{readonlyCount}</span>
              </button>
            </div>
          ) : null}
        </div>

        <div className="ft-toolbar__right">
          {results.emptyFolders.length > 0 ? (
            <label className="search-box ft-search">
              <Search size={14} strokeWidth={1.8} />
              <input
                type="search"
                value={query}
                onChange={(event) => { setQuery(event.target.value); setLimit(PAGE_SIZE) }}
                placeholder="筛选空文件夹"
                aria-label="筛选空文件夹"
              />
            </label>
          ) : null}
        </div>
      </div>

      {results.emptyFolders.length === 0 ? (
        <div className="empty-state ft-empty-compact">
          <span className="ft-empty-icon ft-empty-icon--success" aria-hidden="true">
            <CheckCircle2 size={22} strokeWidth={1.7} />
          </span>
          <h2>没有找到空文件夹。</h2>
          <p>当前扫描范围内的目录均包含文件或子文件夹，无需清理。</p>
        </div>
      ) : filtered.length === 0 ? (
        <div className="empty-state ft-empty-compact">
          <span className="ft-empty-icon" aria-hidden="true">
            <Search size={20} strokeWidth={1.7} />
          </span>
          <h2>没有匹配的空文件夹</h2>
          <p>尝试更改关键字或切换筛选条件。</p>
        </div>
      ) : (
        <>
          <div className="ft-table" role="table" aria-label="空文件夹列表">
            <div className="ft-table__head ft-empty-grid" role="row">
              <span role="columnheader">文件夹位置</span>
              <span role="columnheader">存储位置</span>
              <span className="ft-align-right" role="columnheader">操作</span>
            </div>

            <div className="ft-table__body" role="rowgroup">
              {visible.map((entry) => {
                const mount = mountFor(entry)
                const mountName = mount?.name ?? ''
                const name = entryBasename(entry.path)
                const parent = entryParentPath(entry.path)
                const openable = Boolean(mount && onOpenInFiles && canOpenInFiles?.(mount.id))

                return (
                  <div className="ft-table__row ft-empty-grid" key={entryKey(entry)} role="row">
                    <div className="ft-entry-cell" role="cell">
                      <FileTypeIcon kind="folder" />
                      <div className="ft-entry-cell__text">
                        <span className="ft-entry-name" translate="no" title={entry.path}>{name}</span>
                        <span className="ft-entry-path" translate="no" title={entry.path}>{parent}</span>
                      </div>
                    </div>

                    <div className="ft-mount-cell" role="cell">
                      <span className={`ft-mount-tag${mount?.primary ? ' ft-mount-tag--primary' : ''}`} translate="no">
                        {mountName}
                      </span>
                    </div>

                    <div className="ft-actions-cell" role="cell">
                      {openable && mount && onOpenInFiles ? (
                        <button
                          className="row-action ft-row-action"
                          type="button"
                          aria-label={`打开 ${name} 所在位置`}
                          title="在我的文件中定位"
                          disabled={busy}
                          onClick={() => onOpenInFiles(mount.id, entry.path, true)}
                        >
                          <FolderOpen size={15} strokeWidth={1.8} />
                        </button>
                      ) : null}
                      {entry.removable ? (
                        <button
                          className="danger-button ft-compact-btn"
                          type="button"
                          disabled={busy}
                          onClick={() => onRemove({ target: entry })}
                        >
                          <Trash2 size={13} strokeWidth={1.85} aria-hidden="true" />
                          删除
                        </button>
                      ) : (
                        <span
                          className="ft-pill ft-pill--muted"
                          title="仅主存储且未受保护的空目录支持直接删除"
                        >
                          <Lock size={11} aria-hidden="true" />
                          仅供查看
                        </span>
                      )}
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
                  onClick={() => setLimit((value) => value + PAGE_SIZE)}
                >
                  显示更多
                </button>
                <span className="ft-load-more__meta">剩余 {remaining} 项</span>
              </div>
            ) : null}
          </div>

          <p className="ft-footnote">
            删除前会自动确认目录仍为空且未变化；若删除后父目录变为空，重新扫描即可检出。
          </p>
        </>
      )}
    </section>
  )
}
