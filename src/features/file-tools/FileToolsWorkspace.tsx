import { ArrowLeft, ChevronRight, File, Folder, RefreshCw } from 'lucide-react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Modal } from '../../components/Modal'
import { WorkspaceTabs, type WorkspaceTabItem } from '../../components/WorkspaceTabs'
import { commandErrorMessage, isCommandErrorCode, koofr } from '../../services/koofr'
import type {
  FileToolsEntry,
  FileToolsMount,
  FileToolsProgress,
  FileToolsResults,
} from '../../types/fileTools'
import { formatBytes } from '../../utils/format'
import './fileTools.css'

type Tab = 'usage' | 'empty' | 'duplicates'
type Work = 'scan' | 'verify' | 'remove' | 'browse'
type Removal = { target: FileToolsEntry; keep?: FileToolsEntry }
const tabs: readonly WorkspaceTabItem<Tab>[] = [
  { id: 'usage', label: '空间占用' },
  { id: 'empty', label: '空文件夹' },
  { id: 'duplicates', label: '重复文件' },
]
const initialProgress: FileToolsProgress = { folders: 0, bytes: 0 }
const pageSize = 60

function entryKey(entry: FileToolsEntry) {
  return `${entry.id.scope}/${entry.id.entry}`
}

function ResultList<T>({ items, render }: {
  readonly items: readonly T[]
  readonly render: (item: T) => ReactNode
}) {
  const [limit, setLimit] = useState(pageSize)
  return (
    <>
      {items.slice(0, limit).map(render)}
      {items.length > limit ? (
        <button className="secondary-button" type="button" onClick={() => setLimit(limit + pageSize)}>
          显示更多
        </button>
      ) : null}
    </>
  )
}

interface Props {
  readonly currentMountId: string
  readonly currentPath: string
  readonly onRemoteChange: () => void
}

export function FileToolsWorkspace({ currentMountId, currentPath, onRemoteChange }: Props) {
  const [tab, setTab] = useState<Tab>('usage')
  const [mounts, setMounts] = useState<readonly FileToolsMount[]>([])
  const [sharedUnavailable, setSharedUnavailable] = useState(false)
  const [selectedMounts, setSelectedMounts] = useState<readonly string[]>([])
  const [onlyCurrentFolder, setOnlyCurrentFolder] = useState(false)
  const [mountRevision, setMountRevision] = useState(0)
  const [results, setResults] = useState<FileToolsResults | null>(null)
  const [breadcrumb, setBreadcrumb] = useState<readonly FileToolsEntry[]>([])
  const [children, setChildren] = useState<readonly FileToolsEntry[]>([])
  const [work, setWork] = useState<Work | null>(null)
  const [progress, setProgress] = useState(initialProgress)
  const [removal, setRemoval] = useState<Removal | null>(null)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const run = useRef<string | null>(null)
  const sequence = useRef(0)

  const canScopeToFolder = currentPath !== '/' && selectedMounts.length === 1
    && selectedMounts[0] === currentMountId

  useEffect(() => {
    let active = true
    void koofr.fileToolsMounts().then((loaded) => {
      if (!active) return
      setMounts(loaded.mounts)
      setSharedUnavailable(loaded.sharedUnavailable)
      setSelectedMounts(loaded.mounts.filter((mount) => mount.online).map((mount) => mount.id))
    }).catch((failure: unknown) => {
      if (active) setError(commandErrorMessage(failure, '无法读取存储位置。'))
    })
    return () => { active = false }
  }, [mountRevision])

  useEffect(() => () => {
    sequence.current += 1
    if (run.current) void koofr.closeFileTools(run.current).catch(() => {})
    run.current = null
  }, [])

  async function scan(refresh = false) {
    const request = ++sequence.current
    const scopes = selectedMounts.map((mountId) => ({
      mountId,
      path: canScopeToFolder && onlyCurrentFolder ? currentPath : '/',
    }))
    setError('')
    setNotice('')
    setResults(null)
    setBreadcrumb([])
    setChildren([])
    setProgress(initialProgress)
    setWork('scan')
    try {
      const id = await koofr.beginFileTools()
      if (sequence.current !== request) {
        await koofr.closeFileTools(id)
        return
      }
      run.current = id
      const next = await koofr.scanFileTools(id, scopes, (progress) => {
        if (sequence.current === request) setProgress(progress)
      }, refresh)
      if (sequence.current === request) setResults(next)
    } catch (failure) {
      if (sequence.current === request) setError(commandErrorMessage(failure, '扫描失败。'))
    } finally {
      if (sequence.current === request) setWork(null)
    }
  }

  function cancelScan() {
    sequence.current += 1
    if (run.current) void koofr.closeFileTools(run.current).catch(() => {})
    run.current = null
    setWork(null)
    setNotice('扫描已取消。')
  }

  async function browse(next: readonly FileToolsEntry[]) {
    const id = run.current
    if (!id) return
    if (!next.length) {
      setBreadcrumb([])
      setChildren([])
      return
    }
    setWork('browse')
    setError('')
    try {
      const entries = await koofr.fileToolsUsage(id, next[next.length - 1].id)
      if (run.current !== id) return
      setBreadcrumb(next)
      setChildren(entries)
    } catch (failure) {
      if (run.current === id) setError(commandErrorMessage(failure, '无法显示目录内容。'))
    } finally {
      if (run.current === id) setWork(null)
    }
  }

  async function verify(size: number) {
    const id = run.current
    if (!id) return
    setWork('verify')
    setNotice('')
    setError('')
    setProgress(initialProgress)
    try {
      const next = await koofr.verifyFileTools(id, size, (progress) => {
        if (run.current === id) setProgress(progress)
      })
      if (run.current === id) setResults(next)
    } catch (failure) {
      if (run.current === id) setError(commandErrorMessage(failure, '核验失败。'))
    } finally {
      if (run.current === id) setWork(null)
    }
  }

  async function remove() {
    const id = run.current
    if (!id || !removal) return
    const { target, keep } = removal
    setRemoval(null)
    setWork('remove')
    setNotice('')
    setError('')
    try {
      const next = await koofr.removeFileToolsEntry(id, target.id, keep?.id)
      onRemoteChange()
      if (run.current !== id) return
      setResults(next)
      setBreadcrumb([])
      setChildren([])
      setNotice('已删除选中的项目。')
    } catch (failure) {
      if (run.current === id) {
        setError(isCommandErrorCode(failure, 'conflict')
          ? '文件或目录已变化。请重新扫描后重试。'
          : commandErrorMessage(failure, '无法删除该项目。请重新扫描后重试。'))
      }
    } finally {
      if (run.current === id) setWork(null)
    }
  }

  const mountName = (entry: FileToolsEntry) => results?.mounts[entry.id.scope].name
  const folder = breadcrumb.at(-1)
  const usage = folder ? children : results?.roots ?? []

  return (
    <main className="workspace" aria-label="文件工具">
      <div className="workspace__header">
        <div>
          <h1>文件工具</h1>
          <p className="workspace__description">分析空间、查找空文件夹和重复文件。</p>
        </div>
        <WorkspaceTabs ariaLabel="文件工具" items={tabs} activeId={tab} onChange={setTab} />
      </div>
      <div className="file-tools__body">
        <section className="file-tools__section" aria-label="扫描范围">
          <div className="file-tools__line">
            <strong>扫描范围</strong>
            <button
              className="icon-button icon-button--bordered"
              type="button"
              title="刷新存储位置"
              aria-label="刷新存储位置"
              disabled={work !== null}
              onClick={() => { setError(''); setMountRevision((value) => value + 1) }}
            >
              <RefreshCw size={14} />
            </button>
          </div>
          <div className="file-tools__mounts">
            {mounts.map((mount) => (
              <label key={mount.id}>
                <input
                  type="checkbox"
                  checked={selectedMounts.includes(mount.id)}
                  disabled={work !== null || !mount.online}
                  onChange={(event) => {
                    const checked = event.currentTarget.checked
                    setSelectedMounts((ids) => checked ? [...ids, mount.id] : ids.filter((id) => id !== mount.id))
                  }}
                />
                <span translate="no">{mount.name}</span>
                {!mount.online ? <small>暂时离线</small> : null}
              </label>
            ))}
            {mounts.length === 0 ? <small>没有可用的存储位置</small> : null}
          </div>
          {sharedUnavailable ? <small>共享目录列表暂时无法读取。</small> : null}
          {canScopeToFolder ? (
            <label className="file-tools__scope">
              <input type="checkbox" checked={onlyCurrentFolder} disabled={work !== null}
                onChange={(event) => setOnlyCurrentFolder(event.currentTarget.checked)} />
              <span>仅扫描当前文件夹</span> <span translate="no">{currentPath}</span>
            </label>
          ) : null}
          <div className="file-tools__line">
            <button className="primary-button" type="button"
              disabled={work !== null || selectedMounts.length === 0} onClick={() => void scan()}>
              开始扫描
            </button>
            <button className="secondary-button" type="button"
              disabled={work !== null || selectedMounts.length === 0} onClick={() => void scan(true)}>
              强制从云端扫描
            </button>
            {work === 'scan' ? (
              <>
                <span role="status"><span>已扫描文件夹</span> {progress.folders}</span>
                <button className="secondary-button" type="button" onClick={cancelScan}>取消扫描</button>
              </>
            ) : null}
            {work === 'verify' ? (
              <>
                <span role="status"><span>已读取</span> {formatBytes(progress.bytes)}</span>
                <button className="secondary-button" type="button" onClick={() => {
                  if (run.current) void koofr.cancelFileToolsVerification(run.current).catch(() => {})
                }}>停止核验</button>
              </>
            ) : null}
            {work === 'remove' ? <span role="status">正在删除…</span> : null}
          </div>
        </section>
        {error ? <p className="file-tools__error" role="alert">{error}</p> : null}
        {notice ? <p role="status">{notice}</p> : null}
        {results && results.cachedFolders > 0 ? (
          <p className="workspace__description">
            <span>已复用目录缓存</span> · {results.cachedFolders} <span>个文件夹</span>
          </p>
        ) : null}
        {results?.roots.some((root) => !root.complete) ? (
          <p className="workspace__description">部分目录无法读取或受到保护，结果仅包含已扫描的内容。</p>
        ) : null}

        {results && tab === 'usage' ? (
          <section className="file-tools__section" aria-label="空间占用">
            <div className="file-tools__line">
              {folder ? (
                <>
                  <button className="icon-button icon-button--bordered" type="button" aria-label="上一级"
                    disabled={work !== null} onClick={() => void browse(breadcrumb.slice(0, -1))}>
                    <ArrowLeft size={15} />
                  </button>
                  <strong translate="no">{mountName(folder)} · {folder.path}</strong>
                </>
              ) : <strong>按存储位置统计</strong>}
              {folder ? <span>{formatBytes(folder.size)} · {folder.files} <span>个文件</span></span> : null}
            </div>
            {usage.length === 0 ? <small>没有已扫描的项目。</small> : null}
            <ResultList key={folder ? entryKey(folder) : 'roots'} items={usage} render={(entry) => {
              const Icon = entry.directory ? Folder : File
              const label = folder ? entry.path.split('/').at(-1) : `${mountName(entry)} · ${entry.path}`
              return (
                <div className="file-tools__row" key={entryKey(entry)}>
                  <Icon size={16} aria-hidden="true" />
                  <span className="file-tools__path" title={label} translate="no">{label}</span>
                  {!entry.complete ? <small>统计不完整</small> : null}
                  <span>{formatBytes(entry.complete || entry.directory ? entry.size : null)}</span>
                  {entry.directory ? <small>{entry.files} <span>个文件</span></small> : null}
                  {entry.canBrowse ? (
                    <button className="icon-button" type="button" aria-label="查看目录"
                      disabled={work !== null} onClick={() => void browse([...breadcrumb, entry])}>
                      <ChevronRight size={16} />
                    </button>
                  ) : null}
                </div>
              )
            }} />
          </section>
        ) : null}

        {results && tab === 'empty' ? (
          <section className="file-tools__section" aria-label="空文件夹">
            <strong>空文件夹</strong>
            {results.emptyFolders.length === 0 ? <small>没有找到空文件夹。</small> : null}
            <ResultList items={results.emptyFolders} render={(entry) => (
              <div className="file-tools__row" key={entryKey(entry)}>
                <Folder size={16} aria-hidden="true" />
                <span className="file-tools__path" translate="no">{mountName(entry)} · {entry.path}</span>
                {entry.removable ? (
                  <button className="secondary-button" type="button" disabled={work !== null}
                    onClick={() => setRemoval({ target: entry })}>删除</button>
                ) : <small>仅供查看</small>}
              </div>
            )} />
          </section>
        ) : null}

        {results && tab === 'duplicates' ? (
          <>
            <section className="file-tools__section" aria-label="已确认的重复文件">
              <strong>已确认的重复文件</strong>
              {results.duplicates.length === 0 ? <small>没有已确认的重复文件。</small> : null}
              <ResultList items={results.duplicates} render={(group) => (
                <DuplicateGroup key={entryKey(group[0])} entries={group} mountName={mountName}
                  busy={work !== null} onRemove={setRemoval} />
              )} />
            </section>
            <section className="file-tools__section" aria-label="待核验的候选文件">
              <strong>待核验的候选文件</strong>
              {results.candidates.length === 0 ? <small>没有待核验的候选文件。</small> : null}
              <ResultList items={results.candidates} render={(group) => (
                <div className="file-tools__group" key={group.size}>
                  <div className="file-tools__line">
                    <span>{group.entries.length} <span>个候选文件</span> · {formatBytes(group.size)}</span>
                    <span><span>预计需读取</span> {formatBytes(group.estimatedBytes)}</span>
                    <button className="secondary-button" type="button" disabled={work !== null}
                      onClick={() => void verify(group.size)}>核验这组</button>
                  </div>
                  {group.failedEntries > 0 ? (
                    <small>{group.failedEntries} <span>个文件无法核实，可重新扫描或重试。</span></small>
                  ) : null}
                  <details>
                    <summary>查看候选文件</summary>
                    <ResultList items={group.entries} render={(entry) => (
                      <div className="file-tools__row" key={entryKey(entry)} translate="no">
                        {mountName(entry)} · {entry.path}
                      </div>
                    )} />
                  </details>
                </div>
              )} />
            </section>
          </>
        ) : null}
      </div>
      {removal ? (
        <Modal title={removal.keep ? '删除重复文件？' : '删除空文件夹？'} actionLabel="删除"
          onClose={() => setRemoval(null)} onAction={() => void remove()}
          className="vault-key-modal--danger">
          <p translate="no">{mountName(removal.target)} · {removal.target.path}</p>
          {removal.keep ? <p><span>保留</span> <span translate="no">{removal.keep.path}</span></p> : null}
        </Modal>
      ) : null}
    </main>
  )
}

function DuplicateGroup({ entries, mountName, busy, onRemove }: {
  readonly entries: readonly FileToolsEntry[]
  readonly mountName: (entry: FileToolsEntry) => string | undefined
  readonly busy: boolean
  readonly onRemove: (removal: Removal) => void
}) {
  const [keepKey, setKeepKey] = useState('')
  const keeper = entries.find((entry) => entryKey(entry) === keepKey)
  const canClean = entries.filter((entry) => entry.removable).length >= 2
  return (
    <div className="file-tools__group">
      <div className="file-tools__line">
        <span>{entries.length} <span>个内容相同的文件</span> · {formatBytes(entries[0].size)}</span>
        {canClean ? (
          <label>
            <span>保留文件</span>{' '}
            <select value={keeper ? keepKey : ''} onChange={(event) => setKeepKey(event.target.value)} disabled={busy}>
              <option value="">选择要保留的文件</option>
              {entries.filter((entry) => entry.removable).map((entry) => (
                <option key={entryKey(entry)} value={entryKey(entry)} translate="no">{entry.path}</option>
              ))}
            </select>
          </label>
        ) : <small>仅供查看</small>}
      </div>
      {entries.some((entry) => entry.id.scope !== entries[0].id.scope) ? (
        <small>不同存储位置的结果可能指向同一文件。</small>
      ) : null}
      <ResultList items={entries} render={(entry) => (
        <div className="file-tools__row" key={entryKey(entry)}>
          <File size={16} aria-hidden="true" />
          <span className="file-tools__path" translate="no">{mountName(entry)} · {entry.path}</span>
          {entry.removable && keeper && keeper.id.scope === entry.id.scope && keeper.path.toLowerCase() !== entry.path.toLowerCase() ? (
            <button className="secondary-button" type="button" disabled={busy}
              onClick={() => onRemove({ target: entry, keep: keeper })}>删除</button>
          ) : null}
        </div>
      )} />
    </div>
  )
}
