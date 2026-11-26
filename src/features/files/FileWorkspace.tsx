import {
  ArrowDown,
  ArrowDownToLine,
  ArrowUp,
  Check,
  ChevronRight,
  Copy,
  Eye,
  FileText,
  Folder,
  FolderInput,
  FolderPlus,
  Pencil,
  RefreshCw,
  Search,
  Share2,
  Trash2,
} from 'lucide-react'
import { memo, useDeferredValue, useEffect, useMemo, useState, type MouseEvent as ReactMouseEvent } from 'react'
import { FileTypeIcon } from '../../components/FileTypeIcon'
import { isPreviewableFile } from '../../preview'
import type { KoofrMount, RemoteFile } from '../../types/backend'
import type { UploadMode } from '../../types/files'
import { UploadModeMenu } from './UploadModeMenu'
import {
  fileKind,
  formatBytes,
  formatModified,
  isDirectory,
  pathCrumbs,
} from '../../utils/format'

type SortKey = 'name' | 'modified' | 'size'
type SortDir = 'asc' | 'desc'

interface FileWorkspaceProps {
  mounts: KoofrMount[]
  activeMountId: string
  path: string
  files: RemoteFile[]
  loading: boolean
  error: string
  lastSyncedAt: Date | null
  onMountChange: (mountId: string) => void
  onNavigate: (path: string) => void
  onRefresh: () => void
  onCreateFolder: () => void
  onUpload: (mode: UploadMode) => void
  onDownload: (file: RemoteFile) => void
  onPreview: (file: RemoteFile) => void
  onShare: (file: RemoteFile) => void
  onRename: (file: RemoteFile) => void
  onMove: (files: RemoteFile[]) => void
  onCopy?: (files: RemoteFile[]) => void
  onDelete: (files: RemoteFile[]) => void
}

const EMPTY_SELECTION = new Set<string>()

function directoryHeading(path: string, activeMount?: KoofrMount) {
  if (path === '/') return activeMount?.name || '我的文件'
  return path.split('/').filter(Boolean).at(-1) ?? '我的文件'
}

function parentDirectory(path: string) {
  const segments = path.split('/').filter(Boolean)
  segments.pop()
  return segments.length > 0 ? `/${segments.join('/')}` : '/'
}

export const FileWorkspace = memo(function FileWorkspace({
  mounts,
  activeMountId,
  path,
  files,
  loading,
  error,
  lastSyncedAt,
  onMountChange,
  onNavigate,
  onRefresh,
  onCreateFolder,
  onUpload,
  onDownload,
  onPreview,
  onShare,
  onRename,
  onMove,
  onCopy,
  onDelete,
}: FileWorkspaceProps) {
  const [query, setQuery] = useState('')
  const [sortKey, setSortKey] = useState<SortKey>('name')
  const [sortDir, setSortDir] = useState<SortDir>('asc')
  const [selection, setSelection] = useState<{ scope: string; ids: Set<string>; anchor: string | null }>({
    scope: '',
    ids: EMPTY_SELECTION,
    anchor: null,
  })
  const deferredQuery = useDeferredValue(query)
  const scope = `${activeMountId}:${path}`
  const selectedIds = selection.scope === scope ? selection.ids : EMPTY_SELECTION
  const anchorId = selection.scope === scope ? selection.anchor : null
  const activeMount = mounts.find((mount) => mount.id === activeMountId)

  const toggleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir((current) => (current === 'asc' ? 'desc' : 'asc'))
    } else {
      setSortKey(key)
      setSortDir(key === 'name' ? 'asc' : 'desc')
    }
  }

  const visibleFiles = useMemo(() => {
    const normalizedQuery = deferredQuery.trim().toLocaleLowerCase('zh-CN')
    const matches = normalizedQuery
      ? files.filter((file) => file.name.toLocaleLowerCase('zh-CN').includes(normalizedQuery))
      : files
    const direction = sortDir === 'asc' ? 1 : -1
    return [...matches].sort((left, right) => {
      const directoryOrder = Number(isDirectory(right)) - Number(isDirectory(left))
      if (directoryOrder !== 0) return directoryOrder
      if (sortKey === 'modified') {
        const diff = (left.modified - right.modified) * direction
        if (diff !== 0) return diff
      } else if (sortKey === 'size') {
        const diff = ((left.size ?? 0) - (right.size ?? 0)) * direction
        if (diff !== 0) return diff
      }
      return left.name.localeCompare(right.name, 'zh-CN', { numeric: true }) * direction
    })
  }, [deferredQuery, files, sortDir, sortKey])

  const selectedFiles = useMemo(
    () => files.filter((file) => selectedIds.has(file.path)),
    [files, selectedIds],
  )

  const toggleSelection = (id: string) => {
    setSelection((current) => {
      const next = new Set(current.scope === scope ? current.ids : EMPTY_SELECTION)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return { scope, ids: next, anchor: id }
    })
  }

  const selectRow = (id: string, event: ReactMouseEvent) => {
    if (event.shiftKey && anchorId) {
      const startIndex = visibleFiles.findIndex((file) => file.path === anchorId)
      const endIndex = visibleFiles.findIndex((file) => file.path === id)
      if (startIndex !== -1 && endIndex !== -1) {
        const [from, to] = startIndex <= endIndex ? [startIndex, endIndex] : [endIndex, startIndex]
        const next = new Set(event.ctrlKey || event.metaKey ? selectedIds : EMPTY_SELECTION)
        for (let index = from; index <= to; index += 1) {
          next.add(visibleFiles[index].path)
        }
        setSelection({ scope, ids: next, anchor: anchorId })
        return
      }
    }
    if (event.ctrlKey || event.metaKey) {
      toggleSelection(id)
      return
    }
    setSelection({ scope, ids: new Set([id]), anchor: id })
  }

  const allVisibleSelected = visibleFiles.length > 0
    && visibleFiles.every((file) => selectedIds.has(file.path))
  const toggleAll = () => {
    setSelection((current) => {
      const next = new Set(current.scope === scope ? current.ids : EMPTY_SELECTION)
      visibleFiles.forEach((file) => {
        if (allVisibleSelected) next.delete(file.path)
        else next.add(file.path)
      })
      return { scope, ids: next, anchor: null }
    })
  }

  const singleSelection = selectedFiles.length === 1 ? selectedFiles[0] : null

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target
      if (
        target instanceof HTMLInputElement
        || target instanceof HTMLTextAreaElement
        || target instanceof HTMLSelectElement
        || (target instanceof HTMLElement && target.isContentEditable)
      ) {
        return
      }
      if (document.querySelector('dialog[open]')) return

      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'a') {
        if (visibleFiles.length === 0) return
        event.preventDefault()
        setSelection({
          scope,
          ids: new Set(visibleFiles.map((file) => file.path)),
          anchor: visibleFiles[0]?.path ?? null,
        })
      } else if (event.key === 'F2' && singleSelection) {
        event.preventDefault()
        onRename(singleSelection)
      } else if (event.key === 'Delete' && selectedFiles.length > 0) {
        event.preventDefault()
        onDelete(selectedFiles)
      } else if (event.key === 'Backspace' && path !== '/') {
        event.preventDefault()
        onNavigate(parentDirectory(path))
      }
    }
    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [onDelete, onNavigate, onRename, path, scope, selectedFiles, singleSelection, visibleFiles])
  const previewableSelection = singleSelection && isPreviewableFile(singleSelection)
    ? singleSelection
    : null
  const downloadableSelection = singleSelection
  const crumbs = pathCrumbs(path)
  const syncedText = lastSyncedAt
    ? `更新于 ${lastSyncedAt.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`
    : '尚未更新'

  return (
    <main className="workspace">
      <div className="workspace__topbar workspace__topbar--files">
        <nav className="breadcrumb" aria-label="当前文件夹路径">
          <select
            className="mount-select"
            value={activeMountId}
            aria-label="存储位置"
            disabled={loading || mounts.length === 0}
            onChange={(event) => onMountChange(event.target.value)}
          >
            {mounts.map((mount) => <option translate="no" value={mount.id} key={mount.id}>{mount.name}</option>)}
          </select>
          <span>/</span>
          <button type="button" onClick={() => onNavigate('/')} disabled={path === '/'}>根目录</button>
          {crumbs.map((crumb) => (
            <span className="breadcrumb__segment" key={crumb.path}>
              <span>/</span>
              <button translate="no" type="button" onClick={() => onNavigate(crumb.path)} disabled={crumb.path === path}>
                {crumb.label}
              </button>
            </span>
          ))}
        </nav>
        <label className="search-box">
          <Search size={15} strokeWidth={1.8} />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="筛选当前文件夹"
            aria-label="筛选当前文件夹"
          />
        </label>
        <div className="top-actions">
          <UploadModeMenu disabled={!activeMountId || loading} onSelect={onUpload} />
          <button
            className="secondary-button"
            type="button"
            disabled={!activeMountId || loading}
            onClick={onCreateFolder}
          >
            <FolderPlus size={15} strokeWidth={1.8} />
            新建文件夹
          </button>
          <button className="icon-button icon-button--bordered" type="button" aria-label="刷新当前文件夹" disabled={loading} onClick={onRefresh}>
            <RefreshCw className={loading ? 'icon-spin' : ''} size={15} strokeWidth={1.8} />
          </button>
        </div>
      </div>

      <div className="workspace__heading-row">
        <header className="workspace__heading">
          <h1 translate={path === '/' && !activeMount?.name ? undefined : 'no'}>{directoryHeading(path, activeMount)}</h1>
          <p>{files.length} 个项目 · {loading ? '正在刷新…' : syncedText}</p>
        </header>

        {selectedFiles.length > 0 ? (
          <div className="selection-toolbar" aria-label="所选文件操作">
            <strong>{selectedFiles.length}</strong><span>已选中</span><i />
            <button type="button" disabled={!downloadableSelection} onClick={() => downloadableSelection && onDownload(downloadableSelection)}>
              <ArrowDownToLine size={15} strokeWidth={1.8} />下载
            </button>
            <button type="button" disabled={!previewableSelection} onClick={() => previewableSelection && onPreview(previewableSelection)}>
              <Eye size={15} strokeWidth={1.8} />预览
            </button>
            <button type="button" disabled={!singleSelection} onClick={() => singleSelection && onShare(singleSelection)}>
              <Share2 size={15} strokeWidth={1.8} />分享
            </button>
            <button type="button" disabled={!singleSelection} onClick={() => singleSelection && onRename(singleSelection)}>
              <Pencil size={15} strokeWidth={1.8} />重命名
            </button>
            <button type="button" onClick={() => onMove(selectedFiles)}>
              <FolderInput size={15} strokeWidth={1.8} />移动
            </button>
            {onCopy ? (
              <button type="button" onClick={() => onCopy(selectedFiles)}>
                <Copy size={15} strokeWidth={1.8} />复制
              </button>
            ) : null}
            <button className="selection-action--danger" type="button" onClick={() => onDelete(selectedFiles)}>
              <Trash2 size={15} strokeWidth={1.8} />删除
            </button>
          </div>
        ) : null}
      </div>

      <section
        className={`file-list${selectedFiles.length > 0 ? ' file-list--has-selection' : ''}`}
        role="grid"
        aria-label="文件列表"
        aria-busy={loading}
      >
        <div className="file-row file-row--header" role="row">
          <button
            className={`checkbox${allVisibleSelected ? ' checkbox--checked' : ''}`}
            type="button"
            role="checkbox"
            aria-checked={allVisibleSelected}
            onClick={toggleAll}
            aria-label="全选当前文件"
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
          <span role="columnheader" aria-sort={sortKey === 'modified' ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}>
            <button
              className={`column-sort${sortKey === 'modified' ? ' column-sort--active' : ''}`}
              type="button"
              onClick={() => toggleSort('modified')}
            >
              修改时间
              {sortKey === 'modified' ? (sortDir === 'asc' ? <ArrowUp size={12} aria-hidden="true" /> : <ArrowDown size={12} aria-hidden="true" />) : null}
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

        {path !== '/' ? (
          <div className="file-row file-row--parent" role="row">
            <span role="gridcell" aria-hidden="true" />
            <button className="file-name file-name--button" type="button" onClick={() => onNavigate(parentDirectory(path))}>
              <FileTypeIcon kind="folder" />
              <strong>..</strong>
            </button>
            <span role="gridcell">返回上一级目录</span>
            <span className="file-cell--size" role="gridcell" />
            <button className="row-action" type="button" aria-label="返回上一级目录" onClick={() => onNavigate(parentDirectory(path))}>
              <ArrowUp size={15} strokeWidth={1.8} />
            </button>
          </div>
        ) : null}

        {visibleFiles.map((file) => {
          const kind = fileKind(file)
          const selected = selectedIds.has(file.path)
          const directory = isDirectory(file)
          const previewable = isPreviewableFile(file)
          return (
            <div className={`file-row${selected ? ' file-row--selected' : ''}`} role="row" aria-selected={selected} key={file.path}>
              <button
                className={`checkbox${selected ? ' checkbox--checked' : ''}`}
                type="button"
                role="checkbox"
                aria-checked={selected}
                onClick={() => toggleSelection(file.path)}
                aria-label={`${selected ? '取消选择' : '选择'} ${file.name}`}
              >
                {selected ? <Check size={12} strokeWidth={2.4} /> : null}
              </button>
              <button
                className="file-name file-name--button"
                type="button"
                onDoubleClick={directory ? undefined : () => (previewable ? onPreview(file) : onDownload(file))}
                onClick={(event) => {
                  if (directory && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
                    onNavigate(file.path)
                    return
                  }
                  selectRow(file.path, event)
                }}
              >
                <FileTypeIcon kind={kind} />
                <strong translate="no">{file.name}</strong>
              </button>
              <span role="gridcell">{formatModified(file.modified)}</span>
              <span className="file-cell--size" role="gridcell">{directory ? '-' : formatBytes(file.size)}</span>
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
                <button
                  className="row-action"
                  type="button"
                  aria-label={directory ? `打开 ${file.name}` : `下载 ${file.name}`}
                  onClick={() => directory ? onNavigate(file.path) : onDownload(file)}
                >
                  {directory ? <ChevronRight size={16} strokeWidth={1.8} /> : <ArrowDownToLine size={15} strokeWidth={1.8} />}
                </button>
              )}
            </div>
          )
        })}

        {loading && visibleFiles.length === 0 ? (
          <div className="empty-state">
            <RefreshCw className="icon-spin" size={20} strokeWidth={1.7} />
            <strong>正在读取 Koofr 文件</strong>
            <span>请稍候</span>
          </div>
        ) : null}

        {!loading && error ? (
          <div className="empty-state empty-state--error">
            <FileText size={20} strokeWidth={1.7} />
            <strong>无法显示这个文件夹</strong>
            <span>{error}</span>
            <button className="secondary-button" type="button" onClick={onRefresh}>重试</button>
          </div>
        ) : null}

        {!loading && !error && visibleFiles.length === 0 ? (
          <div className="empty-state">
            {query ? <Search size={20} strokeWidth={1.7} /> : <Folder size={20} strokeWidth={1.7} />}
            <strong>{query ? '没有匹配的项目' : '这个文件夹是空的'}</strong>
            <span>{query ? '试试筛选其他文件名' : '上传文件或新建文件夹以开始使用'}</span>
          </div>
        ) : null}
      </section>
    </main>
  )
})
