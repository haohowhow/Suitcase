import {
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  FileSearch,
  FolderOpen,
  Info,
  LoaderCircle,
  Lock,
  Search,
  ShieldCheck,
  Square,
  Trash2,
  TriangleAlert,
} from 'lucide-react'
import { useDeferredValue, useMemo, useState } from 'react'
import { FileTypeIcon } from '../../components/FileTypeIcon'
import type {
  FileToolsCandidate,
  FileToolsEntry,
  FileToolsProgress,
  FileToolsResults,
} from '../../types/fileTools'
import { fileKindByName, formatBytes } from '../../utils/format'
import {
  cleanableScopesForGroup,
  entryBasename,
  entryKey,
  entryParentPath,
  type FileToolsRemoval,
  type FileToolsSummary,
} from './fileToolsHelpers'

const PAGE_SIZE = 40
type DuplicateFilter = 'all' | 'confirmed' | 'candidates'

interface DuplicatesViewProps {
  readonly results: FileToolsResults
  readonly summary: FileToolsSummary
  readonly busy: boolean
  readonly verifyingSize: number | null
  readonly progress: FileToolsProgress
  readonly onVerify: (size: number) => void
  readonly onCancelVerify: () => void
  readonly onRemove: (removal: FileToolsRemoval) => void
  readonly canOpenInFiles?: (mountId: string) => boolean
  readonly onOpenInFiles?: (mountId: string, path: string, directory: boolean) => void
}

export function DuplicatesView({
  results,
  summary,
  busy,
  verifyingSize,
  progress,
  onVerify,
  onCancelVerify,
  onRemove,
  canOpenInFiles,
  onOpenInFiles,
}: DuplicatesViewProps) {
  const [filter, setFilter] = useState<DuplicateFilter>('all')
  const [query, setQuery] = useState('')
  const [confirmedLimit, setConfirmedLimit] = useState(PAGE_SIZE)
  const [candidateLimit, setCandidateLimit] = useState(PAGE_SIZE)
  const deferredQuery = useDeferredValue(query)

  const normalizedQuery = deferredQuery.trim().toLocaleLowerCase('zh-CN')

  const filteredDuplicates = useMemo(() => {
    if (!normalizedQuery) return results.duplicates
    return results.duplicates.filter((group) =>
      group.some((entry) => {
        const mountName = results.mounts[entry.id.scope]?.name ?? ''
        return entry.path.toLocaleLowerCase('zh-CN').includes(normalizedQuery)
          || mountName.toLocaleLowerCase('zh-CN').includes(normalizedQuery)
      }),
    )
  }, [normalizedQuery, results.duplicates, results.mounts])

  const filteredCandidates = useMemo(() => {
    if (!normalizedQuery) return results.candidates
    return results.candidates.filter((group) =>
      group.entries.some((entry) => {
        const mountName = results.mounts[entry.id.scope]?.name ?? ''
        return entry.path.toLocaleLowerCase('zh-CN').includes(normalizedQuery)
          || mountName.toLocaleLowerCase('zh-CN').includes(normalizedQuery)
      }),
    )
  }, [normalizedQuery, results.candidates, results.mounts])

  const mountName = (entry: FileToolsEntry) => results.mounts[entry.id.scope]?.name ?? ''
  const mountIdFor = (entry: FileToolsEntry) => results.mounts[entry.id.scope]?.id ?? ''

  const showConfirmed = filter === 'all' || filter === 'confirmed'
  const showCandidates = filter === 'all' || filter === 'candidates'
  const totalGroups = results.duplicates.length + results.candidates.length

  return (
    <div className="ft-duplicates">
      <div className="ft-toolbar ft-toolbar--panel">
        <div className="ft-toolbar__left">
          <div className="ft-filter-chips" role="group" aria-label="筛选重复文件分组">
            <button
              className={`sync-filter-chip${filter === 'all' ? ' sync-filter-chip--active' : ''}`}
              type="button"
              onClick={() => setFilter('all')}
            >
              全部 <span>{totalGroups}</span>
            </button>
            <button
              className={`sync-filter-chip${filter === 'confirmed' ? ' sync-filter-chip--active' : ''}`}
              type="button"
              onClick={() => setFilter('confirmed')}
            >
              已确认重复 <span>{results.duplicates.length}</span>
            </button>
            <button
              className={`sync-filter-chip${results.candidates.length > 0 ? ' sync-filter-chip--warning' : ''}${filter === 'candidates' ? ' sync-filter-chip--active' : ''}`}
              type="button"
              onClick={() => setFilter('candidates')}
            >
              待核验候选 <span>{results.candidates.length}</span>
            </button>
          </div>
          {summary.removableReclaimableBytes > 0 ? (
            <span className="ft-pill ft-pill--accent">
              最多可释放 {formatBytes(summary.removableReclaimableBytes)}
            </span>
          ) : null}
        </div>

        <div className="ft-toolbar__right">
          {totalGroups > 0 ? (
            <label className="search-box ft-search">
              <Search size={14} strokeWidth={1.8} />
              <input
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="筛选重复文件或路径"
                aria-label="筛选重复文件或路径"
              />
            </label>
          ) : null}
        </div>
      </div>

      {verifyingSize !== null ? (
        <div className="ft-verify-banner" role="status">
          <LoaderCircle size={16} className="spin" aria-hidden="true" />
          <div className="ft-verify-banner__copy">
            <strong>正在读取并比对文件内容…</strong>
            <span>
              <span>已读取</span> {formatBytes(progress.bytes)}
            </span>
          </div>
          <button
            className="secondary-button ft-compact-btn"
            type="button"
            onClick={onCancelVerify}
          >
            <Square size={11} strokeWidth={2.2} aria-hidden="true" />
            停止核验
          </button>
        </div>
      ) : null}

      {showConfirmed ? (
        <section className="ft-section-block" aria-label="已确认的重复文件">
          <div className="ft-section-head">
            <div className="ft-section-head__title">
              <h2>已确认的重复文件</h2>
              <span className="ft-count-pill">{filteredDuplicates.length}</span>
            </div>
            <p className="ft-section-head__desc">
              内容已完全一致。先在组内选定一份要保留的副本，即可安全删除同存储内的其余副本。
            </p>
          </div>

          {filteredDuplicates.length === 0 ? (
            <div className="ft-section-empty">
              <CheckCircle2 size={16} aria-hidden="true" />
              <span>{normalizedQuery ? '没有匹配的已确认重复文件。' : '没有已确认的重复文件。'}</span>
            </div>
          ) : (
            <div className="ft-card-stack">
              {filteredDuplicates.slice(0, confirmedLimit).map((group) => (
                <DuplicateGroupCard
                  key={entryKey(group[0])}
                  entries={group}
                  mountName={mountName}
                  mountIdFor={mountIdFor}
                  busy={busy}
                  onRemove={onRemove}
                  canOpenInFiles={canOpenInFiles}
                  onOpenInFiles={onOpenInFiles}
                />
              ))}
              {filteredDuplicates.length > confirmedLimit ? (
                <div className="ft-load-more">
                  <button
                    className="secondary-button"
                    type="button"
                    onClick={() => setConfirmedLimit((value) => value + PAGE_SIZE)}
                  >
                    显示更多
                  </button>
                  <span className="ft-load-more__meta">
                    剩余 {filteredDuplicates.length - confirmedLimit} 组
                  </span>
                </div>
              ) : null}
            </div>
          )}
        </section>
      ) : null}

      {showCandidates ? (
        <section className="ft-section-block" aria-label="待核验的候选文件">
          <div className="ft-section-head">
            <div className="ft-section-head__title">
              <h2>待核验的候选文件</h2>
              <span className="ft-count-pill">{filteredCandidates.length}</span>
            </div>
            <p className="ft-section-head__desc">
              这些文件大小相同但缺少可比对的原生哈希。点击“核验这组”才会读取内容计算校验和。
            </p>
          </div>

          {filteredCandidates.length === 0 ? (
            <div className="ft-section-empty">
              <ShieldCheck size={16} aria-hidden="true" />
              <span>{normalizedQuery ? '没有匹配的待核验候选文件。' : '没有待核验的候选文件。'}</span>
            </div>
          ) : (
            <div className="ft-card-stack">
              {filteredCandidates.slice(0, candidateLimit).map((group) => (
                <CandidateGroupCard
                  key={group.size}
                  group={group}
                  mountName={mountName}
                  mountIdFor={mountIdFor}
                  busy={busy}
                  verifying={verifyingSize === group.size}
                  onVerify={onVerify}
                  canOpenInFiles={canOpenInFiles}
                  onOpenInFiles={onOpenInFiles}
                />
              ))}
              {filteredCandidates.length > candidateLimit ? (
                <div className="ft-load-more">
                  <button
                    className="secondary-button"
                    type="button"
                    onClick={() => setCandidateLimit((value) => value + PAGE_SIZE)}
                  >
                    显示更多
                  </button>
                  <span className="ft-load-more__meta">
                    剩余 {filteredCandidates.length - candidateLimit} 组
                  </span>
                </div>
              ) : null}
            </div>
          )}
        </section>
      ) : null}
    </div>
  )
}

interface DuplicateGroupCardProps {
  readonly entries: readonly FileToolsEntry[]
  readonly mountName: (entry: FileToolsEntry) => string
  readonly mountIdFor: (entry: FileToolsEntry) => string
  readonly busy: boolean
  readonly onRemove: (removal: FileToolsRemoval) => void
  readonly canOpenInFiles?: (mountId: string) => boolean
  readonly onOpenInFiles?: (mountId: string, path: string, directory: boolean) => void
}

function DuplicateGroupCard({
  entries,
  mountName,
  mountIdFor,
  busy,
  onRemove,
  canOpenInFiles,
  onOpenInFiles,
}: DuplicateGroupCardProps) {
  const [keepKey, setKeepKey] = useState('')
  const [rowLimit, setRowLimit] = useState(PAGE_SIZE)

  const cleanableScopes = useMemo(() => cleanableScopesForGroup(entries), [entries])
  const canClean = cleanableScopes.size > 0
  const keeper = entries.find(
    (entry) => entryKey(entry) === keepKey && entry.removable && cleanableScopes.has(entry.id.scope),
  )
  const crossMount = entries.some((entry) => entry.id.scope !== entries[0].id.scope)
  const sampleName = entryBasename(entries[0].path)
  const size = entries[0].size
  const overhead = size * Math.max(0, entries.length - 1)
  const visibleEntries = entries.slice(0, rowLimit)

  return (
    <article className="ft-group-card">
      <header className="ft-group-card__header">
        <div className="ft-group-card__identity">
          <FileTypeIcon kind={fileKindByName(sampleName)} />
          <div className="ft-group-card__meta">
            <div className="ft-group-card__title-row">
              <strong>
                {entries.length} <span>个内容相同的文件</span>
              </strong>
              <span className="ft-size-badge">{formatBytes(size)}</span>
              {overhead > 0 ? (
                <span className="ft-overhead-text">冗余 {formatBytes(overhead)}</span>
              ) : null}
            </div>
            <span className="ft-group-card__sample" translate="no" title={sampleName}>
              {sampleName}
            </span>
          </div>
        </div>

        <div className="ft-group-card__status">
          {canClean ? (
            keeper ? (
              <span className="ft-pill ft-pill--accent">
                <Check size={11} strokeWidth={2.4} aria-hidden="true" />
                已选定保留副本
              </span>
            ) : (
              <span className="ft-pill ft-pill--subtle">请选择要保留的副本</span>
            )
          ) : (
            <span className="ft-pill ft-pill--muted" title="跨存储或外部共享中的重复文件当前仅供查看">
              <Lock size={11} aria-hidden="true" />
              仅供查看
            </span>
          )}
        </div>
      </header>

      {crossMount ? (
        <div className="ft-group-notice">
          <Info size={13} aria-hidden="true" />
          <span>不同存储位置的结果可能指向同一文件。</span>
        </div>
      ) : null}

      <div className="ft-group-rows">
        {visibleEntries.map((entry) => {
          const key = entryKey(entry)
          const isKeeper = keeper ? entryKey(keeper) === key : false
          const canBeKeeper = entry.removable && cleanableScopes.has(entry.id.scope)
          const canDeleteThis = Boolean(
            entry.removable
            && keeper
            && keeper.id.scope === entry.id.scope
            && keeper.path.toLowerCase() !== entry.path.toLowerCase(),
          )
          const name = entryBasename(entry.path)
          const parent = entryParentPath(entry.path)
          const mountId = mountIdFor(entry)
          const openable = Boolean(mountId && onOpenInFiles && canOpenInFiles?.(mountId))

          return (
            <div
              className={`ft-dup-row${isKeeper ? ' ft-dup-row--keeper' : ''}`}
              key={key}
            >
              {canClean ? (
                <div className="ft-dup-row__selector">
                  {canBeKeeper ? (
                    <button
                      type="button"
                      role="radio"
                      aria-checked={isKeeper}
                      className={`ft-keep-radio${isKeeper ? ' ft-keep-radio--active' : ''}`}
                      disabled={busy}
                      onClick={() => setKeepKey(isKeeper ? '' : key)}
                    >
                      <span className="ft-keep-radio__dot" aria-hidden="true">
                        {isKeeper ? <Check size={10} strokeWidth={3} /> : null}
                      </span>
                      <span>{isKeeper ? '保留此份' : '设为保留'}</span>
                    </button>
                  ) : (
                    <span className="ft-keep-placeholder" aria-hidden="true">—</span>
                  )}
                </div>
              ) : null}

              <div className="ft-dup-row__info">
                <div className="ft-dup-row__name-line">
                  <span className="ft-entry-name" translate="no" title={entry.path}>{name}</span>
                  <span className="ft-mount-tag" translate="no">{mountName(entry)}</span>
                </div>
                <span className="ft-entry-path" translate="no" title={entry.path}>{parent}</span>
              </div>

              <div className="ft-dup-row__actions">
                {openable && onOpenInFiles ? (
                  <button
                    className="row-action ft-row-action"
                    type="button"
                    aria-label={`打开 ${name} 所在位置`}
                    title="在我的文件中定位"
                    disabled={busy}
                    onClick={() => onOpenInFiles(mountId, entry.path, false)}
                  >
                    <FolderOpen size={15} strokeWidth={1.8} />
                  </button>
                ) : null}
                {isKeeper ? (
                  <span className="ft-pill ft-pill--accent">保留</span>
                ) : canDeleteThis && keeper ? (
                  <button
                    className="danger-button ft-compact-btn"
                    type="button"
                    disabled={busy}
                    onClick={() => onRemove({ target: entry, keep: keeper })}
                  >
                    <Trash2 size={13} strokeWidth={1.85} aria-hidden="true" />
                    删除
                  </button>
                ) : !entry.removable ? (
                  <span className="ft-pill ft-pill--muted">仅供查看</span>
                ) : null}
              </div>
            </div>
          )
        })}

        {entries.length > rowLimit ? (
          <div className="ft-load-more ft-load-more--inset">
            <button
              className="secondary-button"
              type="button"
              onClick={() => setRowLimit((value) => value + PAGE_SIZE)}
            >
              显示更多
            </button>
          </div>
        ) : null}
      </div>
    </article>
  )
}

interface CandidateGroupCardProps {
  readonly group: FileToolsCandidate
  readonly mountName: (entry: FileToolsEntry) => string
  readonly mountIdFor: (entry: FileToolsEntry) => string
  readonly busy: boolean
  readonly verifying: boolean
  readonly onVerify: (size: number) => void
  readonly canOpenInFiles?: (mountId: string) => boolean
  readonly onOpenInFiles?: (mountId: string, path: string, directory: boolean) => void
}

function CandidateGroupCard({
  group,
  mountName,
  mountIdFor,
  busy,
  verifying,
  onVerify,
  canOpenInFiles,
  onOpenInFiles,
}: CandidateGroupCardProps) {
  const [expanded, setExpanded] = useState(false)
  const [rowLimit, setRowLimit] = useState(PAGE_SIZE)
  const sampleName = group.entries[0] ? entryBasename(group.entries[0].path) : ''

  return (
    <article className="ft-group-card ft-group-card--candidate">
      <header className="ft-group-card__header">
        <div className="ft-group-card__identity">
          <FileTypeIcon kind={sampleName ? fileKindByName(sampleName) : 'file'} />
          <div className="ft-group-card__meta">
            <div className="ft-group-card__title-row">
              <strong>
                {group.entries.length} <span>个候选文件</span>
              </strong>
              <span className="ft-size-badge">{formatBytes(group.size)}</span>
              <span className="ft-estimate-pill">
                <span>预计需读取</span> <strong>{formatBytes(group.estimatedBytes)}</strong>
              </span>
            </div>
            {sampleName ? (
              <span className="ft-group-card__sample" translate="no" title={sampleName}>
                {sampleName}
              </span>
            ) : null}
          </div>
        </div>

        <div className="ft-group-card__controls">
          <button
            className="button button--subtle"
            type="button"
            aria-expanded={expanded}
            onClick={() => setExpanded((value) => !value)}
          >
            {expanded ? <ChevronUp size={14} aria-hidden="true" /> : <ChevronDown size={14} aria-hidden="true" />}
            <span>{expanded ? '收起候选文件' : '查看候选文件'}</span>
          </button>
          <button
            className="secondary-button ft-compact-btn"
            type="button"
            disabled={busy}
            onClick={() => onVerify(group.size)}
          >
            {verifying ? (
              <LoaderCircle size={13} className="spin" aria-hidden="true" />
            ) : (
              <FileSearch size={13} strokeWidth={1.85} aria-hidden="true" />
            )}
            <span>{verifying ? '正在核验…' : '核验这组'}</span>
          </button>
        </div>
      </header>

      {group.failedEntries > 0 ? (
        <div className="ft-group-notice ft-group-notice--warning">
          <TriangleAlert size={13} aria-hidden="true" />
          <span>{group.failedEntries} <span>个文件无法核实，可重新扫描或重试。</span></span>
        </div>
      ) : null}

      {expanded ? (
        <div className="ft-group-rows">
          {group.entries.slice(0, rowLimit).map((entry) => {
            const name = entryBasename(entry.path)
            const parent = entryParentPath(entry.path)
            const mountId = mountIdFor(entry)
            const openable = Boolean(mountId && onOpenInFiles && canOpenInFiles?.(mountId))

            return (
              <div className="ft-dup-row" key={entryKey(entry)}>
                <div className="ft-dup-row__info">
                  <div className="ft-dup-row__name-line">
                    <span className="ft-entry-name" translate="no" title={entry.path}>{name}</span>
                    <span className="ft-mount-tag" translate="no">{mountName(entry)}</span>
                  </div>
                  <span className="ft-entry-path" translate="no" title={entry.path}>{parent}</span>
                </div>
                {openable && onOpenInFiles ? (
                  <div className="ft-dup-row__actions">
                    <button
                      className="row-action ft-row-action"
                      type="button"
                      aria-label={`打开 ${name} 所在位置`}
                      title="在我的文件中定位"
                      disabled={busy}
                      onClick={() => onOpenInFiles(mountId, entry.path, false)}
                    >
                      <FolderOpen size={15} strokeWidth={1.8} />
                    </button>
                  </div>
                ) : null}
              </div>
            )
          })}
          {group.entries.length > rowLimit ? (
            <div className="ft-load-more ft-load-more--inset">
              <button
                className="secondary-button"
                type="button"
                onClick={() => setRowLimit((value) => value + PAGE_SIZE)}
              >
                显示更多
              </button>
            </div>
          ) : null}
        </div>
      ) : null}
    </article>
  )
}
