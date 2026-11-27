import {
  ArrowRight,
  Check,
  Cloud,
  CloudDownload,
  CopyCheck,
  Database,
  Folder,
  FolderX,
  PieChart,
  RefreshCw,
  ScanSearch,
  TriangleAlert,
} from 'lucide-react'
import type { FileToolsMount, FileToolsResults } from '../../types/fileTools'
import { formatBytes } from '../../utils/format'
import type { FileToolId, FileToolsSummary } from './fileToolsHelpers'

interface ToolEntryHubProps {
  readonly mounts: readonly FileToolsMount[]
  readonly mountsLoading: boolean
  readonly sharedUnavailable: boolean
  readonly selectedMounts: readonly string[]
  readonly onToggleMount: (mountId: string, checked: boolean) => void
  readonly onSelectAllOnline: () => void
  readonly onSelectPrimaryOnly: () => void
  readonly onRefreshMounts: () => void
  readonly canScopeToFolder: boolean
  readonly currentPath: string
  readonly onlyCurrentFolder: boolean
  readonly onOnlyCurrentFolderChange: (checked: boolean) => void
  readonly preferredTool: FileToolId
  readonly onSelectPreferredTool: (tool: FileToolId) => void
  readonly results: FileToolsResults | null
  readonly summary: FileToolsSummary | null
  readonly onOpenTool: (tool: FileToolId) => void
  readonly onScan: (refresh: boolean, targetTool?: FileToolId) => void
  readonly busy: boolean
}

interface ToolCardSpec {
  readonly id: FileToolId
  readonly title: string
  readonly description: string
  readonly capability: string
  readonly tone: 'usage' | 'empty' | 'duplicates'
  readonly icon: typeof PieChart
}

const TOOL_CARDS: readonly ToolCardSpec[] = [
  {
    id: 'usage',
    title: '空间占用',
    description: '按目录逐层统计体积占比与文件数量，快速定位大文件夹与大文件。',
    capability: '支持逐层钻取与定位',
    tone: 'usage',
    icon: PieChart,
  },
  {
    id: 'empty',
    title: '空文件夹',
    description: '找出不含任何内容的目录，支持安全删除主存储中的空目录。',
    capability: '自动跳过保险箱与分卷包',
    tone: 'empty',
    icon: FolderX,
  },
  {
    id: 'duplicates',
    title: '重复文件',
    description: '通过内容哈希精确比对重复副本，跨云候选支持按需内容校验。',
    capability: '删除前强制指定保留副本',
    tone: 'duplicates',
    icon: CopyCheck,
  },
]

export function ToolEntryHub({
  mounts,
  mountsLoading,
  sharedUnavailable,
  selectedMounts,
  onToggleMount,
  onSelectAllOnline,
  onSelectPrimaryOnly,
  onRefreshMounts,
  canScopeToFolder,
  currentPath,
  onlyCurrentFolder,
  onOnlyCurrentFolderChange,
  preferredTool,
  onSelectPreferredTool,
  results,
  summary,
  onOpenTool,
  onScan,
  busy,
}: ToolEntryHubProps) {
  const onlineMounts = mounts.filter((mount) => mount.online)
  const hasPrimaryOnline = onlineMounts.some((mount) => mount.primary)
  const allOnlineSelected = onlineMounts.length > 0 && selectedMounts.length === onlineMounts.length
  const primaryOnlySelected = hasPrimaryOnline
    && selectedMounts.length === onlineMounts.filter((mount) => mount.primary).length
    && selectedMounts.every((id) => onlineMounts.some((mount) => mount.id === id && mount.primary))
  const canStartScan = !busy && selectedMounts.length > 0

  const renderCardMetric = (id: FileToolId) => {
    if (!summary) return null
    if (id === 'usage') {
      return (
        <div className="ft-tool-card__metric">
          <strong>{formatBytes(summary.totalBytes)}</strong>
          <span>{summary.totalFiles} <span>个文件</span></span>
        </div>
      )
    }
    if (id === 'empty') {
      return (
        <div className="ft-tool-card__metric">
          <strong>{summary.emptyCount} <span>个空文件夹</span></strong>
          <span>
            {summary.emptyCount === 0
              ? '目录整洁，无需清理'
              : summary.removableEmptyCount > 0
                ? `${summary.removableEmptyCount} 个可安全清理`
                : '当前结果仅供查看'}
          </span>
        </div>
      )
    }
    return (
      <div className="ft-tool-card__metric">
        <strong>
          {summary.duplicateGroupCount} <span>组已确认</span>
        </strong>
        <span>
          {summary.removableReclaimableBytes > 0
            ? `最多可释放 ${formatBytes(summary.removableReclaimableBytes)}`
            : summary.candidateGroupCount > 0
              ? `另有 ${summary.candidateGroupCount} 组待核验`
              : '未发现重复文件'}
        </span>
      </div>
    )
  }

  return (
    <div className="ft-hub">
      <section className="ft-hub__tools" aria-label="可用工具">
        <div className="ft-hub__section-head">
          <div>
            <h2>选择工具</h2>
            <p>
              {results
                ? '一次扫描会同时生成三项分析结果，点击任意卡片即可进入查看或清理。'
                : '一次扫描即可完成空间分析、空目录检索与重复核查；先选择你关心的工具和扫描范围。'}
            </p>
          </div>
        </div>

        <div className="ft-tool-grid">
          {TOOL_CARDS.map((card) => {
            const Icon = card.icon
            const isSelected = !results && preferredTool === card.id
            return (
              <button
                key={card.id}
                type="button"
                className={`ft-tool-card ft-tool-card--${card.tone}${isSelected ? ' ft-tool-card--selected' : ''}`}
                aria-pressed={!results ? isSelected : undefined}
                onClick={() => {
                  if (results) {
                    onOpenTool(card.id)
                  } else {
                    onSelectPreferredTool(card.id)
                  }
                }}
              >
                <div className="ft-tool-card__top">
                  <span className="ft-tool-card__icon" aria-hidden="true">
                    <Icon size={18} strokeWidth={1.85} />
                  </span>
                  {isSelected ? (
                    <span className="ft-pill ft-pill--accent">
                      <Check size={11} strokeWidth={2.4} aria-hidden="true" />
                      扫描后打开
                    </span>
                  ) : results ? (
                    <span className="ft-tool-card__open">
                      <span>进入查看</span>
                      <ArrowRight size={13} strokeWidth={2} aria-hidden="true" />
                    </span>
                  ) : null}
                </div>

                <div className="ft-tool-card__body">
                  <strong>{card.title}</strong>
                  <p>{card.description}</p>
                </div>

                {results ? (
                  renderCardMetric(card.id)
                ) : (
                  <div className="ft-tool-card__footer">
                    <span>{card.capability}</span>
                  </div>
                )}
              </button>
            )
          })}
        </div>
      </section>

      <section className="ft-scope-card" aria-label="扫描范围">
        <div className="ft-scope-card__header">
          <div className="ft-scope-card__title">
            <h2>扫描范围</h2>
            <span className="ft-scope-card__count">
              已选 {selectedMounts.length} / {onlineMounts.length} 个在线位置
            </span>
          </div>

          <div className="ft-scope-card__controls">
            {onlineMounts.length > 1 ? (
              <div className="ft-scope-presets" role="group" aria-label="快捷选择范围">
                <button
                  type="button"
                  className={`sync-filter-chip${allOnlineSelected ? ' sync-filter-chip--active' : ''}`}
                  disabled={busy}
                  onClick={onSelectAllOnline}
                >
                  全部在线
                </button>
                {hasPrimaryOnline ? (
                  <button
                    type="button"
                    className={`sync-filter-chip${primaryOnlySelected ? ' sync-filter-chip--active' : ''}`}
                    disabled={busy}
                    onClick={onSelectPrimaryOnly}
                  >
                    仅主存储
                  </button>
                ) : null}
              </div>
            ) : null}

            <button
              className="icon-button icon-button--bordered"
              type="button"
              title="刷新存储位置"
              aria-label="刷新存储位置"
              disabled={busy || mountsLoading}
              onClick={onRefreshMounts}
            >
              <RefreshCw className={mountsLoading ? 'icon-spin' : ''} size={14} strokeWidth={1.85} />
            </button>
          </div>
        </div>

        <div className="ft-scope-card__content">
          {mountsLoading && mounts.length === 0 ? (
            <div className="ft-mounts-empty" role="status">
              <RefreshCw className="icon-spin" size={15} aria-hidden="true" />
              <span>正在读取存储位置…</span>
            </div>
          ) : mounts.length === 0 ? (
            <div className="ft-mounts-empty">
              <span>没有可用的存储位置</span>
            </div>
          ) : (
            <div className="ft-mount-grid">
              {mounts.map((mount) => {
                const checked = selectedMounts.includes(mount.id)
                const disabled = busy || !mount.online
                return (
                  <label
                    key={mount.id}
                    className={`ft-mount-card${checked ? ' ft-mount-card--selected' : ''}${!mount.online ? ' ft-mount-card--offline' : ''}`}
                  >
                    <input
                      type="checkbox"
                      className="ft-mount-card__input"
                      aria-label={mount.name}
                      checked={checked}
                      disabled={disabled}
                      onChange={(event) => onToggleMount(mount.id, event.currentTarget.checked)}
                    />
                    <span className={`ft-mount-card__check${checked ? ' ft-mount-card__check--checked' : ''}`} aria-hidden="true">
                      {checked ? <Check size={11} strokeWidth={2.8} /> : null}
                    </span>
                    <span className={`ft-mount-card__icon${mount.primary ? ' ft-mount-card__icon--primary' : ''}`} aria-hidden="true">
                      {mount.primary ? <Database size={15} strokeWidth={1.8} /> : <Cloud size={15} strokeWidth={1.8} />}
                    </span>
                    <span className="ft-mount-card__meta">
                      <span className="ft-mount-card__name-row">
                        <strong translate="no" title={mount.name}>{mount.name}</strong>
                        {mount.primary ? <span className="place-card__pill">主存储</span> : null}
                      </span>
                      <span className={`place-card__status${mount.online ? ' place-card__status--online' : ''}`}>
                        <span className="place-card__dot" aria-hidden="true" />
                        {mount.online ? (mount.primary ? '在线 · 支持清理' : '在线 · 仅供查看与核验') : '暂时离线'}
                      </span>
                    </span>
                  </label>
                )
              })}
            </div>
          )}

          {canScopeToFolder ? (
            <label className={`ft-folder-scope${onlyCurrentFolder ? ' ft-folder-scope--active' : ''}`}>
              <input
                type="checkbox"
                className="ft-mount-card__input"
                checked={onlyCurrentFolder}
                disabled={busy}
                onChange={(event) => onOnlyCurrentFolderChange(event.currentTarget.checked)}
              />
              <span className={`ft-mount-card__check${onlyCurrentFolder ? ' ft-mount-card__check--checked' : ''}`} aria-hidden="true">
                {onlyCurrentFolder ? <Check size={11} strokeWidth={2.8} /> : null}
              </span>
              <Folder size={15} strokeWidth={1.8} className="ft-folder-scope__icon" aria-hidden="true" />
              <span className="ft-folder-scope__text">
                <span>仅扫描当前文件夹</span>
                <code className="ft-path-chip" translate="no" title={currentPath}>{currentPath}</code>
              </span>
            </label>
          ) : null}

          {sharedUnavailable ? (
            <div className="ft-scope-warning" role="status">
              <TriangleAlert size={14} aria-hidden="true" />
              <span>共享目录列表暂时无法读取。</span>
            </div>
          ) : null}
        </div>

        <div className="ft-scope-card__footer">
          <p className="ft-scope-card__note">
            优先复用本地目录缓存以加快扫描；私人保险箱与分卷上传包会自动跳过。
          </p>
          <div className="ft-scope-card__actions">
            <button
              className="secondary-button"
              type="button"
              disabled={!canStartScan}
              onClick={() => onScan(true)}
            >
              <CloudDownload size={14} strokeWidth={1.85} aria-hidden="true" />
              强制从云端扫描
            </button>
            <button
              className="primary-button"
              type="button"
              disabled={!canStartScan}
              onClick={() => onScan(false)}
            >
              <ScanSearch size={14} strokeWidth={1.9} aria-hidden="true" />
              开始扫描
            </button>
          </div>
        </div>
      </section>
    </div>
  )
}
