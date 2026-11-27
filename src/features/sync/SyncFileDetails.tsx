import { useDeferredValue, useEffect, useState } from 'react'
import { commandErrorMessage, koofr } from '../../services/koofr'
import type { SyncFileFilter, SyncFilePage, SyncFileStatus, SyncIssue, SyncPairSummary } from '../../types/backend'

const fileStates: Record<SyncFileStatus['state'], string> = {
  in_sync: '两端一致',
  metadata_current: '云端信息已更新',
  unchecked: '待核对',
  queued: '等待中',
  running: '进行中',
  waiting: '等待网络',
  failed: '需要处理',
}

const operations: Record<NonNullable<SyncFileStatus['operation']>, string> = {
  mkdir_local: '创建本地文件夹',
  mkdir_remote: '创建云端文件夹',
  upload: '上传',
  download: '下载',
  update_placeholder: '更新占位符',
  delete_local: '删除本地项',
  delete_remote: '删除云端项',
  move_remote: '移动云端项',
  conflict: '创建冲突副本',
  type_conflict: '处理类型冲突',
  sync: '同步操作',
  scan: '同步扫描',
  read_file: '下载文件内容',
  read_directory: '读取目录',
}

const availabilityText: Record<NonNullable<SyncFileStatus['availability']>, string> = {
  local: '本机可用',
  online_only: '仅在线',
  unexpanded: '尚未读取此目录',
}


function issueGuidance(issue: SyncIssue) {
  switch (issue.code) {
    case 'not_authenticated':
      return '请重新登录 Koofr，然后重试。'
    case 'network_error':
    case 'network_stalled':
      return '请检查网络连接，然后重试。'
    case 'disk_full':
      return '请释放本机磁盘空间，然后重试。'
    case 'sync_root_unavailable':
      return '请确认同步文件夹及其磁盘可以访问，然后继续同步。'
    case 'forbidden':
      return '请检查本地和云端文件夹的访问权限。'
    case 'integrity_error':
      return '文件完整性校验失败，请重试。'
    case 'local_io_error':
      return '请检查文件是否被占用，以及本机磁盘是否可用。'
    default:
      return '问题持续时，请记录下方错误码。'
  }
}

export function SyncFileIssues({ pair, path, onShowAll, onRecheck, onResume, onOpenRoot }: {
  readonly pair: SyncPairSummary
  readonly path: string | null
  readonly onShowAll: () => void
  readonly onRecheck: () => Promise<void>
  readonly onResume: () => Promise<void>
  readonly onOpenRoot: () => Promise<void>
}) {
  const [issues, setIssues] = useState<SyncIssue[] | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    let disposed = false
    let loading = false
    const refresh = async () => {
      if (loading) return
      loading = true
      try {
        const data = await koofr.listSyncIssues(pair.id)
        if (!disposed) {
          setIssues(data)
          setError('')
        }
      } catch (failure) {
        if (!disposed) setError(commandErrorMessage(failure, '无法读取同步问题'))
      } finally {
        loading = false
      }
    }
    void refresh()
    const timer = window.setInterval(() => void refresh(), 1500)
    return () => {
      disposed = true
      window.clearInterval(timer)
    }
  }, [pair.id])

  const handleAction = async (action: () => Promise<void>) => {
    setBusy(true)
    try {
      await action()
    } finally {
      setBusy(false)
    }
  }

  const groupOffline = path === null && pair.status === 'offline'
  const groupNeedsLogin = pair.lastErrorCode === 'not_authenticated'
  const shown = issues?.filter((issue) => path === null || issue.relativePath === path)
  return (
    <div className="sync-detail-pane">
      {path !== null ? (
        <div className="sync-file-toolbar">
          <span translate="no">{path}</span>
          <button className="button button--subtle" type="button" onClick={onShowAll}>查看全部问题</button>
        </div>
      ) : null}
      {groupOffline ? (
        <div className="sync-file-toolbar">
          <span>{groupNeedsLogin ? '请重新登录 Koofr，然后重试。' : '请检查网络连接，然后重试。'}</span>
          {!groupNeedsLogin ? (
            <button className="button button--subtle" type="button" disabled={busy}
              onClick={() => void handleAction(onRecheck)}>重新检查此同步组</button>
          ) : null}
        </div>
      ) : null}
      {error ? <p role="alert" className="sync-file-hint">{error}</p> : null}
      {shown?.length ? (
        <ul className="sync-file-list" aria-label="同步问题">
          {shown.map((issue) => {
            const nativeRead = issue.operation === 'read_file' || issue.operation === 'read_directory'
            const resume = pair.paused
            const needsLogin = issue.code === 'not_authenticated'
            return (
              <li className="sync-file-row" key={issue.id}>
                <strong translate={issue.relativePath ? 'no' : undefined}>{issue.relativePath || '同步位置'}</strong>
                <span>{issue.message}</span>
                <span>{issueGuidance(issue)}</span>
                {nativeRead ? <span>{issue.operation === 'read_file' ? '请在资源管理器重新打开文件。' : '请在资源管理器重新打开目录。'}</span> : null}
                <small><code translate="no">{issue.code}</code>{' · '}{new Date(issue.updatedAt ?? issue.createdAt).toLocaleString()}</small>
                {!needsLogin ? (
                  <button className="button button--subtle" type="button" disabled={busy}
                    onClick={() => void handleAction(resume ? onResume : nativeRead ? onOpenRoot : onRecheck)}>
                    {resume ? '继续同步' : nativeRead ? '打开目录' : '重新检查此同步组'}
                  </button>
                ) : null}
              </li>
            )
          })}
        </ul>
      ) : !error && !groupOffline ? (
        <p role="status" className="sync-file-hint">{shown ? '当前没有未解决问题。' : '正在读取同步问题…'}</p>
      ) : null}
    </div>
  )
}

export function SyncFileDetails({ pair, onShowIssues }: {
  readonly pair: SyncPairSummary
  readonly onShowIssues: (path: string) => void
}) {
  const [search, setSearch] = useState('')
  const query = useDeferredValue(search.trim())
  const [filter, setFilter] = useState<SyncFileFilter>('all')
  const [page, setPage] = useState(0)
  const [result, setResult] = useState<{ key: string; data?: SyncFilePage; error?: string }>()
  const requestKey = JSON.stringify([query, filter, page])

  useEffect(() => {
    let disposed = false
    let loading = false
    const refresh = async () => {
      if (loading) return
      loading = true
      try {
        const data = await koofr.listSyncFiles(pair.id, query, filter, page)
        if (!disposed) setResult({ key: requestKey, data })
      } catch (error) {
        if (!disposed) setResult({ key: requestKey, error: commandErrorMessage(error, '无法读取文件同步状态') })
      } finally {
        loading = false
      }
    }
    void refresh()
    const timer = window.setInterval(() => void refresh(), 1500)
    return () => {
      disposed = true
      window.clearInterval(timer)
    }
  }, [pair.id, query, filter, page, requestKey])

  const displayed = result?.key === requestKey ? result : undefined

  return (
    <div className="sync-detail-pane">
      <div className="sync-file-toolbar">
        <input
          type="search"
          aria-label="搜索文件名或相对路径"
          placeholder="搜索文件名或相对路径"
          value={search}
          maxLength={1024}
          onChange={(event) => { setSearch(event.currentTarget.value); setPage(0) }}
        />
        <select
          aria-label="筛选文件状态"
          value={filter}
          onChange={(event) => { setFilter(event.currentTarget.value as SyncFileFilter); setPage(0) }}
        >
          <option value="all">全部文件</option>
          <option value="pending">等待或进行中</option>
          <option value="failed">需要处理</option>
          <option value="checked">已核对</option>
        </select>
      </div>
      {pair.syncMode === 'onDemand' ? (
        <p className="sync-file-hint">资源管理器展开目录后，才能在此查询其中的文件。</p>
      ) : null}
      {pair.paused || pair.status === 'offline' ? (
        <p className="sync-file-hint">
          {pair.paused ? '此同步组已暂停。' : '此同步组在等待连接。'}
          文件状态基于最近一次检查。
        </p>
      ) : null}
      {displayed?.error ? (
        <p role="alert" className="sync-file-hint">{displayed.error}</p>
      ) : !displayed?.data ? (
        <p role="status" className="sync-file-hint">正在读取文件同步状态…</p>
      ) : (
        <>
          <ul className="sync-file-list" aria-label="文件同步状态">
            {displayed.data.files.map((file) => (
              <li className="sync-file-row" key={file.relativePath}>
                <strong translate="no" title={file.relativePath}>{file.relativePath}</strong>
                <span>
                  {file.operation ? <>{operations[file.operation]} · </> : null}{fileStates[file.state]}
                  {file.checkedAt ? <>{' · '}<span>最近核对</span>{' '}{new Date(file.checkedAt).toLocaleString()}</> : null}
                </span>
                {pair.syncMode === 'onDemand' ? (
                  <small>
                    {file.availability ? availabilityText[file.availability] : file.kind === 'file' ? '本地状态待确认' : ''}
                    {file.pinned ? <> · <span>始终保留</span></> : null}
                    {file.availabilityCheckedAt ? <>{' · '}{new Date(file.availabilityCheckedAt).toLocaleString()}</> : null}
                    {file.lastReadAt ? <>{' · '}<span>上次成功下载</span>{' '}{new Date(file.lastReadAt).toLocaleString()}</> : null}
                  </small>
                ) : null}
                {file.state === 'failed' ? (
                  <button className="button button--subtle" type="button" onClick={() => onShowIssues(file.relativePath)}>查看问题</button>
                ) : null}
              </li>
            ))}
          </ul>
          {!displayed.data.files.length ? <p className="sync-file-hint">没有符合条件的文件。</p> : null}
        </>
      )}
      {page > 0 || displayed?.data?.hasMore ? (
        <nav className="sync-file-pagination" aria-label="文件列表分页">
          <button className="button button--subtle" type="button" disabled={page === 0} onClick={() => setPage(page - 1)}>上一页</button>
          <span>{page + 1}</span>
          <button className="button button--subtle" type="button" disabled={!displayed?.data?.hasMore} onClick={() => setPage(page + 1)}>下一页</button>
        </nav>
      ) : null}
    </div>
  )
}
