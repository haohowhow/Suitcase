import {
  ArrowDown,
  ArrowDownToLine,
  ArrowUp,
  Check,
  Clock3,
  Eye,
  FileText,
  FolderOpen,
  Palette,
  RefreshCw,
  RotateCcw,
  Search,
  Share2,
  Trash2,
} from 'lucide-react'
import { memo, useDeferredValue, useMemo, useState, type MouseEvent as ReactMouseEvent } from 'react'
import { FileTypeIcon } from '../../components/FileTypeIcon'
import { isPreviewableFile } from '../../preview'
import type { LocatedFile, RemoteFile, TrashItem } from '../../types/backend'
import type { CollectionView } from './useKoofrCollections'
import { fileKind, formatBytes, formatModified, isDirectory } from '../../utils/format'

type CollectionSortKey = 'name' | 'time' | 'size'
type CollectionSortDir = 'asc' | 'desc'

interface CollectionWorkspaceProps {
  view: CollectionView
  files: LocatedFile[]
  trash: TrashItem[]
  retentionDays: number
  loading: boolean
  error: string
  diagnostic: string
  lastSyncedAt: Date | null
  busy: boolean
  onRefresh: () => void
  onThemeOpen: () => void
  onOpenLocation: (file: LocatedFile) => void
  onDownload: (file: LocatedFile) => void
  onPreview: (file: LocatedFile) => void
  onRestore: (items: TrashItem[]) => void
  onRestoreAll: () => void
  onEmptyTrash: () => void
}

const EMPTY_SELECTION = new Set<string>()

const viewDetails = {
  最近的文件: {
    icon: Clock3,
    description: '按最近修改时间展示账户中的文件和文件夹',
    empty: '还没有最近的文件',
  },
  已共享: {
    icon: Share2,
    description: '我已共享以及其他人共享给我的项目',
    empty: '还没有共享项目',
  },
  回收站: {
    icon: Trash2,
    description: '恢复误删项目，或永久清空回收站',
    empty: '回收站是空的',
  },
} satisfies Record<CollectionView, { icon: typeof Clock3; description: string; empty: string }>

const DELETED_DATE_FORMATTER = new Intl.DateTimeFormat('zh-CN', {
  year: 'numeric',
  month: '2-digit',
  day: '2-digit',
  hour: '2-digit',
  minute: '2-digit',
  hour12: false,
})

function locatedId(file: LocatedFile) {
  return `${file.mountId}:${file.path}`
}

function trashAsFile(item: TrashItem): RemoteFile {
  return {
    name: item.name,
    entryType: 'file',
    modified: 0,
    size: item.size,
    contentType: item.contentType,
    hash: '',
    path: item.path,
  }
}

function formatDeleted(value: string) {
  const normalized = value.trim()
  if (/^\d+$/.test(normalized)) {
    return formatModified(Number(normalized))
  }

  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return '—'
  return DELETED_DATE_FORMATTER.format(date)
}

function parentPath(path: string) {
  const segments = path.split('/').filter(Boolean)
  if (segments.length <= 1) return '/'
  return `/${segments.slice(0, -1).join('/')}`
}

function locationLabel(file: LocatedFile) {
  const directory = isDirectory(file)
  const location = directory ? file.path : parentPath(file.path)
  return location === '/' ? file.mountName || '根目录' : `${file.mountName || '存储位置'}${location}`
}

export const CollectionWorkspace = memo(function CollectionWorkspace({
  view,
  files,
  trash,
  retentionDays,
  loading,
  error,
  diagnostic,
  lastSyncedAt,
  busy,
  onRefresh,
  onThemeOpen,
  onOpenLocation,
  onDownload,
  onPreview,
  onRestore,
  onRestoreAll,
  onEmptyTrash,
}: CollectionWorkspaceProps) {
  const [query, setQuery] = useState('')
  const [sortKey, setSortKey] = useState<CollectionSortKey>('time')
  const [sortDir, setSortDir] = useState<CollectionSortDir>('desc')
  const [selection, setSelection] = useState<{ scope: CollectionView; ids: Set<string>; anchor: string | null }>({
    scope: view,
    ids: EMPTY_SELECTION,
    anchor: null,
  })
  const deferredQuery = useDeferredValue(query)
  const selectedIds = selection.scope === view ? selection.ids : EMPTY_SELECTION
  const anchorId = selection.scope === view ? selection.anchor : null
  const details = viewDetails[view]
  const HeadingIcon = details.icon
  const isTrash = view === '回收站'

  const toggleSort = (key: CollectionSortKey) => {
    if (sortKey === key) {
      setSortDir((current) => (current === 'asc' ? 'desc' : 'asc'))
    } else {
      setSortKey(key)
      setSortDir(key === 'name' ? 'asc' : 'desc')
    }
  }

  const visibleFiles = useMemo(() => {
    const normalized = deferredQuery.trim().toLocaleLowerCase('zh-CN')
    const matched = normalized
      ? files.filter((file) => file.name.toLocaleLowerCase('zh-CN').includes(normalized))
      : files
    const direction = sortDir === 'asc' ? 1 : -1
    return [...matched].sort((left, right) => {
      if (sortKey === 'time') {
        const diff = (left.modified - right.modified) * direction
        if (diff !== 0) return diff
      } else if (sortKey === 'size') {
        const diff = ((left.size ?? 0) - (right.size ?? 0)) * direction
        if (diff !== 0) return diff
      }
      return left.name.localeCompare(right.name, 'zh-CN', { numeric: true }) * direction
    })
  }, [deferredQuery, files, sortDir, sortKey])

  const visibleTrash = useMemo(() => {
    const normalized = deferredQuery.trim().toLocaleLowerCase('zh-CN')
    const matched = normalized
      ? trash.filter((item) => item.name.toLocaleLowerCase('zh-CN').includes(normalized))
      : trash
    const direction = sortDir === 'asc' ? 1 : -1
    return [...matched].sort((left, right) => {
      if (sortKey === 'time') {
        const diff = left.deleted.localeCompare(right.deleted) * direction
        if (diff !== 0) return diff
      } else if (sortKey === 'size') {
        const diff = ((left.size ?? 0) - (right.size ?? 0)) * direction
        if (diff !== 0) return diff
      }
      return left.name.localeCompare(right.name, 'zh-CN', { numeric: true }) * direction
    })
  }, [deferredQuery, sortDir, sortKey, trash])

  const selectedFiles = useMemo(
    () => files.filter((file) => selectedIds.has(locatedId(file))),
    [files, selectedIds],
  )
  const selectedTrash = useMemo(
    () => trash.filter((item) => selectedIds.has(item.versionId)),
    [selectedIds, trash],
  )
  const visibleIds = useMemo(
    () => (isTrash ? visibleTrash.map((item) => item.versionId) : visibleFiles.map(locatedId)),
    [isTrash, visibleFiles, visibleTrash],
  )
  const allVisibleSelected = visibleIds.length > 0 && visibleIds.every((id) => selectedIds.has(id))

  const toggleSelection = (id: string) => {
    setSelection((current) => {
      const next = new Set(current.scope === view ? current.ids : EMPTY_SELECTION)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return { scope: view, ids: next, anchor: id }
    })
  }

  const selectRow = (id: string, event: ReactMouseEvent) => {
    if (event.shiftKey && anchorId) {
      const startIndex = visibleIds.indexOf(anchorId)
      const endIndex = visibleIds.indexOf(id)
      if (startIndex !== -1 && endIndex !== -1) {
        const [from, to] = startIndex <= endIndex ? [startIndex, endIndex] : [endIndex, startIndex]
        const next = new Set(event.ctrlKey || event.metaKey ? selectedIds : EMPTY_SELECTION)
        for (let index = from; index <= to; index += 1) {
          next.add(visibleIds[index])
        }
        setSelection({ scope: view, ids: next, anchor: anchorId })
        return
      }
    }
    if (event.ctrlKey || event.metaKey) {
      toggleSelection(id)
      return
    }
    setSelection({ scope: view, ids: new Set([id]), anchor: id })
  }

  const toggleAll = () => {
    setSelection((current) => {
      const next = new Set(current.scope === view ? current.ids : EMPTY_SELECTION)
      visibleIds.forEach((id) => {
        if (allVisibleSelected) next.delete(id)
        else next.add(id)
      })
      return { scope: view, ids: next, anchor: null }
    })
  }

  const singleFile = selectedFiles.length === 1 ? selectedFiles[0] : null
  const previewableFile = singleFile && isPreviewableFile(singleFile)
    ? singleFile
    : null
  const canOpenLocation = singleFile
    && (isDirectory(singleFile) || singleFile.path !== '/' || singleFile.shareDirection === null)
  const itemCount = isTrash ? trash.length : files.length
  const syncedText = lastSyncedAt
    ? `更新于 ${lastSyncedAt.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`
    : '尚未更新'

  return (
    <main className="workspace collection-workspace">
      <div className="workspace__topbar collection-workspace__topbar">
        <label className="search-box collection-search">
          <Search size={15} strokeWidth={1.8} />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={`筛选${view}`}
            aria-label={`筛选${view}`}
          />
        </label>
        <div className="top-actions">
          {isTrash && trash.length > 0 ? (
            <>
              <button className="secondary-button" type="button" disabled={busy || loading} onClick={onRestoreAll}>
                <RotateCcw size={15} strokeWidth={1.8} />恢复全部
              </button>
              <button className="danger-button" type="button" disabled={busy || loading} onClick={onEmptyTrash}>
                <Trash2 size={15} strokeWidth={1.8} />永久清空
              </button>
            </>
          ) : null}
          <button className="icon-button icon-button--bordered" type="button" aria-label={`刷新${view}`} disabled={loading || busy} onClick={onRefresh}>
            <RefreshCw className={loading ? 'icon-spin' : ''} size={15} strokeWidth={1.8} />
          </button>
          <button className="icon-button icon-button--bordered theme-button-mobile" type="button" aria-label="外观主题" onClick={onThemeOpen}>
            <Palette size={15} strokeWidth={1.8} />
          </button>
        </div>
      </div>

      <div className="workspace__heading-row">
        <header className="workspace__heading">
          <h1>{view}</h1>
          <p>
            {details.description} · {itemCount} 个项目 · {loading ? '正在刷新…' : syncedText}
            {isTrash && retentionDays > 0 ? ` · 保留 ${retentionDays} 天` : ''}
          </p>
        </header>

        {isTrash && selectedTrash.length > 0 ? (
          <div className="selection-toolbar" aria-label="所选回收站项目操作">
            <strong>{selectedTrash.length}</strong><span>已选中</span><i />
            <button type="button" disabled={busy} onClick={() => onRestore(selectedTrash)}>
              <RotateCcw size={15} strokeWidth={1.8} />恢复所选
            </button>
          </div>
        ) : !isTrash && selectedFiles.length > 0 ? (
          <div className="selection-toolbar" aria-label="所选文件操作">
            <strong>{selectedFiles.length}</strong><span>已选中</span><i />
            <button type="button" disabled={!singleFile} onClick={() => singleFile && onDownload(singleFile)}>
              <ArrowDownToLine size={15} strokeWidth={1.8} />下载
            </button>
            <button type="button" disabled={!previewableFile} onClick={() => previewableFile && onPreview(previewableFile)}>
              <Eye size={15} strokeWidth={1.8} />预览
            </button>
            <button type="button" disabled={!canOpenLocation} onClick={() => singleFile && onOpenLocation(singleFile)}>
              <FolderOpen size={15} strokeWidth={1.8} />打开位置
            </button>
          </div>
        ) : null}
      </div>

      <section
        className={`file-list collection-list${(isTrash ? selectedTrash.length : selectedFiles.length) > 0 ? ' file-list--has-selection' : ''}`}
        role="grid"
        aria-label={`${view}列表`}
        aria-busy={loading}
      >
        <div className="file-row file-row--header collection-row" role="row">
          <button
            className={`checkbox${allVisibleSelected ? ' checkbox--checked' : ''}`}
            type="button"
            role="checkbox"
            aria-checked={allVisibleSelected}
            onClick={toggleAll}
            aria-label={`全选${view}`}
          >
            {allVisibleSelected ? <Check size={12} strokeWidth={2.4} /> : null}
          </button>
          <span role="columnheader" aria-sort={sortKey === 'name' ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}>
            <button
              className={`column-sort${sortKey === 'name' ? ' column-sort--active' : ''}`}
              type="button"
              onClick={() => toggleSort('name')}
            >
              名称
              {sortKey === 'name' ? (sortDir === 'asc' ? <ArrowUp size={12} aria-hidden="true" /> : <ArrowDown size={12} aria-hidden="true" />) : null}
            </button>
          </span>
          <span role="columnheader">位置</span>
          <span role="columnheader" aria-sort={sortKey === 'time' ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}>
            <button
              className={`column-sort${sortKey === 'time' ? ' column-sort--active' : ''}`}
              type="button"
              onClick={() => toggleSort('time')}
            >
              {isTrash ? '删除时间' : '修改时间'}
              {sortKey === 'time' ? (sortDir === 'asc' ? <ArrowUp size={12} aria-hidden="true" /> : <ArrowDown size={12} aria-hidden="true" />) : null}
            </button>
          </span>
          <span className="file-cell--size" role="columnheader" aria-sort={sortKey === 'size' ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}>
            <button
              className={`column-sort${sortKey === 'size' ? ' column-sort--active' : ''}`}
              type="button"
              onClick={() => toggleSort('size')}
            >
              大小
              {sortKey === 'size' ? (sortDir === 'asc' ? <ArrowUp size={12} aria-hidden="true" /> : <ArrowDown size={12} aria-hidden="true" />) : null}
            </button>
          </span>
          <span role="columnheader" aria-hidden="true" />
        </div>

        {!isTrash ? visibleFiles.map((file) => {
          const id = locatedId(file)
          const selected = selectedIds.has(id)
          const directory = isDirectory(file)
          const previewable = isPreviewableFile(file)
          const direction = file.shareDirection === 'received' ? '共享给我' : file.shareDirection === 'outgoing' ? '我已共享' : null
          return (
            <div className={`file-row collection-row${selected ? ' file-row--selected' : ''}`} role="row" aria-selected={selected} key={id}>
              <button className={`checkbox${selected ? ' checkbox--checked' : ''}`} type="button" role="checkbox" aria-checked={selected} onClick={() => toggleSelection(id)} aria-label={`${selected ? '取消选择' : '选择'} ${file.name}`}>
                {selected ? <Check size={12} strokeWidth={2.4} /> : null}
              </button>
              <button className="file-name file-name--button" type="button" onClick={(event) => selectRow(id, event)} onDoubleClick={() => directory ? onOpenLocation(file) : previewable ? onPreview(file) : onDownload(file)}>
                <FileTypeIcon kind={fileKind(file)} />
                <strong translate="no">{file.name}</strong>
              </button>
              <span className="collection-location" role="gridcell">{direction ? <em>{direction}</em> : null}<span translate="no">{locationLabel(file)}</span></span>
              <span role="gridcell">{formatModified(file.modified)}</span>
              <span className="file-cell--size" role="gridcell">{directory ? '—' : formatBytes(file.size)}</span>
              {previewable ? (
                <div className="row-actions" role="gridcell">
                  <button className="row-action" type="button" aria-label={`预览 ${file.name}`} onClick={() => onPreview(file)}>
                    <Eye size={15} strokeWidth={1.8} />
                  </button>
                  <button className="row-action" type="button" aria-label={`下载 ${file.name}`} onClick={() => onDownload(file)}>
                    <ArrowDownToLine size={15} strokeWidth={1.8} />
                  </button>
                </div>
              ) : (
                <button className="row-action" type="button" aria-label={directory ? `打开 ${file.name}` : `下载 ${file.name}`} onClick={() => directory ? onOpenLocation(file) : onDownload(file)}>
                  {directory ? <FolderOpen size={15} strokeWidth={1.8} /> : <ArrowDownToLine size={15} strokeWidth={1.8} />}
                </button>
              )}
            </div>
          )
        }) : visibleTrash.map((item) => {
          const selected = selectedIds.has(item.versionId)
          const file = trashAsFile(item)
          const kind = fileKind(file)
          return (
            <div className={`file-row collection-row${selected ? ' file-row--selected' : ''}`} role="row" aria-selected={selected} key={item.versionId}>
              <button className={`checkbox${selected ? ' checkbox--checked' : ''}`} type="button" role="checkbox" aria-checked={selected} onClick={() => toggleSelection(item.versionId)} aria-label={`${selected ? '取消选择' : '选择'} ${item.name}`}>
                {selected ? <Check size={12} strokeWidth={2.4} /> : null}
              </button>
              <button className="file-name file-name--button" type="button" onClick={(event) => selectRow(item.versionId, event)}>
                <FileTypeIcon kind={kind} />
                <strong translate="no">{item.name}</strong>
              </button>
              <span className="collection-location" role="gridcell" translate="no">{item.mountName || '存储位置'}{parentPath(item.path)}</span>
              <span role="gridcell">{formatDeleted(item.deleted)}</span>
              <span className="file-cell--size" role="gridcell">{formatBytes(item.size)}</span>
              <button className="row-action" type="button" aria-label={`恢复 ${item.name}`} disabled={busy} onClick={() => onRestore([item])}>
                <RotateCcw size={15} strokeWidth={1.8} />
              </button>
            </div>
          )
        })}

        {loading && (isTrash ? visibleTrash.length === 0 : visibleFiles.length === 0) ? (
          <div className="empty-state">
            <RefreshCw className="icon-spin" size={20} strokeWidth={1.7} />
            <strong>正在读取{view}</strong>
            <span>请稍候</span>
          </div>
        ) : null}

        {!loading && error ? (
          <div className="empty-state empty-state--error">
            <FileText size={20} strokeWidth={1.7} />
            <strong>无法显示{view}</strong>
            <span>{error}</span>
            {diagnostic ? <code className="error-diagnostic">错误代码：{diagnostic}</code> : null}
            <button className="secondary-button" type="button" onClick={onRefresh}>重试</button>
          </div>
        ) : null}

        {!loading && !error && (isTrash ? visibleTrash.length === 0 : visibleFiles.length === 0) ? (
          <div className="empty-state">
            {query ? <Search size={20} strokeWidth={1.7} /> : <HeadingIcon size={20} strokeWidth={1.7} />}
            <strong>{query ? '没有匹配的项目' : details.empty}</strong>
            <span>{query ? '试试筛选其他文件名' : isTrash ? '删除的项目会在这里显示' : '有新内容时会显示在这里'}</span>
          </div>
        ) : null}
      </section>
    </main>
  )
})
