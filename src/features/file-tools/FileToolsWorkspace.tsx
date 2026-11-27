import {
  ArrowLeft,
  ArrowRight,
  CloudDownload,
  CopyCheck,
  Database,
  Folder,
  FolderX,
  LoaderCircle,
  PieChart,
  RefreshCw,
  ShieldCheck,
  SlidersHorizontal,
  Trash2,
  TriangleAlert,
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { FileTypeIcon } from '../../components/FileTypeIcon'
import { Modal } from '../../components/Modal'
import { WorkspaceTabs, type WorkspaceTabItem } from '../../components/WorkspaceTabs'
import { useFileToolsManager, type FileToolsController } from '../../hooks/useFileToolsManager'
import type { KoofrMount } from '../../types/backend'
import type { FileToolsEntry } from '../../types/fileTools'
import { fileKindByName, formatBytes } from '../../utils/format'
import { DuplicatesView } from './DuplicatesView'
import { EmptyFoldersView } from './EmptyFoldersView'
import {
  entryBasename,
  type FileToolId,
  type FileToolsRemoval,
} from './fileToolsHelpers'
import { SpaceUsageView } from './SpaceUsageView'
import { ToolEntryHub } from './ToolEntryHub'
import './fileTools.css'

interface FileToolsWorkspaceProps {
  readonly currentMountId: string
  readonly currentPath: string
  readonly onRemoteChange: () => void
  readonly controller?: FileToolsController
  readonly workspaceMounts?: readonly KoofrMount[]
  readonly onOpenLocation?: (mountId: string, path: string, directory: boolean) => void
}

export function FileToolsWorkspace(props: FileToolsWorkspaceProps) {
  if (props.controller) {
    return <FileToolsWorkspaceView {...props} controller={props.controller} />
  }
  return <StandaloneFileToolsWorkspace {...props} />
}

function StandaloneFileToolsWorkspace(props: FileToolsWorkspaceProps) {
  const controller = useFileToolsManager({
    authState: 'signedIn',
    onRemoteChange: props.onRemoteChange,
  })
  return <FileToolsWorkspaceView {...props} controller={controller} />
}

interface FileToolsWorkspaceViewProps extends FileToolsWorkspaceProps {
  readonly controller: FileToolsController
}

function FileToolsWorkspaceView({
  currentMountId,
  currentPath,
  controller,
  workspaceMounts,
  onOpenLocation,
}: FileToolsWorkspaceViewProps) {
  const [removal, setRemoval] = useState<FileToolsRemoval | null>(null)

  const {
    view,
    preferredTool,
    mounts,
    mountsLoading,
    sharedUnavailable,
    selectedMounts,
    onlyCurrentFolder,
    results,
    summary,
    breadcrumb,
    children,
    work,
    scanRefresh,
    activeScanScopes,
    verifyingSize,
    progress,
    notice,
    error,
    setView,
    setPreferredTool,
    ensureMountsLoaded,
    refreshMounts,
    toggleMount,
    selectAllOnline,
    selectPrimaryOnly,
    setOnlyCurrentFolder,
    scan,
    cancelScan,
    browse,
    verify,
    cancelVerification,
    remove,
  } = controller

  useEffect(() => {
    ensureMountsLoaded()
  }, [ensureMountsLoaded])

  const canScopeToFolder = currentPath !== '/'
    && selectedMounts.length === 1
    && selectedMounts[0] === currentMountId

  const browsableMountIds = useMemo(
    () => new Set((workspaceMounts ?? []).filter((mount) => mount.online).map((mount) => mount.id)),
    [workspaceMounts],
  )

  const canOpenInFiles = useCallback(
    (mountId: string) => Boolean(onOpenLocation && browsableMountIds.has(mountId)),
    [browsableMountIds, onOpenLocation],
  )

  const toolTabs = useMemo<readonly WorkspaceTabItem<FileToolId>[]>(() => [
    { id: 'usage', label: '空间占用' },
    { id: 'empty', label: '空文件夹', badge: results?.emptyFolders.length },
    {
      id: 'duplicates',
      label: '重复文件',
      badge: results ? results.duplicates.length + results.candidates.length : undefined,
    },
  ], [results])

  const triggerScan = (refresh = false, targetTool?: FileToolId) => {
    void scan({ refresh, targetTool, currentMountId, currentPath })
  }

  const confirmRemove = () => {
    if (!removal) return
    const targetRemoval = removal
    setRemoval(null)
    void remove(targetRemoval)
  }

  const mountName = (entry: FileToolsEntry) => results?.mounts[entry.id.scope]?.name ?? ''
  const inToolView = view !== 'hub' && results !== null && summary !== null && work !== 'scan'

  return (
    <main className="workspace ft-workspace" aria-label="文件工具">
      {inToolView ? (
        <div className="workspace__topbar ft-results-topbar">
          <button
            className="secondary-button ft-back-button"
            type="button"
            disabled={work !== null}
            onClick={() => setView('hub')}
          >
            <ArrowLeft size={14} strokeWidth={1.85} aria-hidden="true" />
            <span>工具入口</span>
          </button>

          <WorkspaceTabs
            ariaLabel="文件工具"
            items={toolTabs}
            activeId={view}
            onChange={(nextTool) => {
              setPreferredTool(nextTool)
              setView(nextTool)
            }}
          />

          <div className="top-actions">
            <button
              className="icon-button icon-button--bordered"
              type="button"
              aria-label="重新扫描"
              title="重新扫描（复用目录缓存）"
              disabled={work !== null || selectedMounts.length === 0}
              onClick={() => triggerScan(false, view)}
            >
              <RefreshCw size={15} strokeWidth={1.85} />
            </button>
            <button
              className="secondary-button"
              type="button"
              title="忽略缓存，重新从云端读取"
              disabled={work !== null || selectedMounts.length === 0}
              onClick={() => triggerScan(true, view)}
            >
              <CloudDownload size={14} strokeWidth={1.85} aria-hidden="true" />
              <span>强制从云端扫描</span>
            </button>
          </div>
        </div>
      ) : (
        <div className="workspace__header">
          <div>
            <h1>文件工具</h1>
            <p className="workspace__description">分析空间、查找空文件夹和重复文件。</p>
          </div>
          {results && work !== 'scan' ? (
            <div className="top-actions">
              <button
                className="secondary-button"
                type="button"
                disabled={work !== null}
                onClick={() => setView(preferredTool)}
              >
                <span>返回扫描结果</span>
                <ArrowRight size={14} strokeWidth={1.85} aria-hidden="true" />
              </button>
            </div>
          ) : null}
        </div>
      )}

      <div className="ft-body">
        {error ? <div className="notice notice--error ft-notice" role="alert">{error}</div> : null}
        {notice ? <div className="notice notice--info ft-notice" role="status">{notice}</div> : null}
        {work === 'remove' ? (
          <div className="ft-verify-banner" role="status">
            <LoaderCircle size={15} className="spin" aria-hidden="true" />
            <span>正在删除所选项目…</span>
          </div>
        ) : null}

        {work === 'scan' ? (
          <section className="ft-scan-stage" aria-label="扫描进度">
            <div className="ft-scan-stage__card">
              <span className="ft-scan-stage__icon" aria-hidden="true">
                <LoaderCircle size={24} strokeWidth={1.85} className="spin" />
              </span>
              <h2>{scanRefresh ? '正在从云端重新扫描…' : '正在扫描云端目录…'}</h2>
              <div className="ft-scan-stage__counter" role="status">
                <span>已扫描文件夹</span>
                <strong>{progress.folders}</strong>
              </div>
              {activeScanScopes.length > 0 ? (
                <div className="ft-scan-stage__scopes">
                  {activeScanScopes.map((scope) => (
                    <span className="ft-mount-tag" key={`${scope.mountId}:${scope.path}`} translate="no">
                      <Database size={11} aria-hidden="true" />
                      {scope.path === '/' ? scope.name : `${scope.name} · ${scope.path}`}
                    </span>
                  ))}
                </div>
              ) : null}
              <p className="ft-scan-stage__hint">
                扫描仅读取目录结构，不下载文件内容；切换其他页面不会中断扫描，保险箱与分卷包自动跳过。
              </p>
              <button className="secondary-button" type="button" onClick={cancelScan}>
                取消扫描
              </button>
            </div>
          </section>
        ) : inToolView ? (
          <>
            <section className="ft-summary-strip" aria-label="扫描结果总览">
              <div className="ft-summary-strip__metrics">
                <button
                  type="button"
                  className={`ft-metric${view === 'usage' ? ' ft-metric--active' : ''}`}
                  onClick={() => { setPreferredTool('usage'); setView('usage') }}
                >
                  <span className="ft-metric__label">
                    <PieChart size={12} strokeWidth={2} aria-hidden="true" />
                    <span>空间占用</span>
                  </span>
                  <strong className="ft-metric__value">{formatBytes(summary.totalBytes)}</strong>
                  <small className="ft-metric__sub">{summary.totalFiles} <span>个文件</span></small>
                </button>

                <button
                  type="button"
                  className={`ft-metric${view === 'empty' ? ' ft-metric--active' : ''}`}
                  onClick={() => { setPreferredTool('empty'); setView('empty') }}
                >
                  <span className="ft-metric__label">
                    <FolderX size={12} strokeWidth={2} aria-hidden="true" />
                    <span>空文件夹</span>
                  </span>
                  <strong className="ft-metric__value">{summary.emptyCount}</strong>
                  <small className="ft-metric__sub">
                    {summary.emptyCount === 0
                      ? '无需清理'
                      : summary.removableEmptyCount > 0
                        ? `${summary.removableEmptyCount} 个可安全清理`
                        : '仅供查看'}
                  </small>
                </button>

                <button
                  type="button"
                  className={`ft-metric${view === 'duplicates' ? ' ft-metric--active' : ''}`}
                  onClick={() => { setPreferredTool('duplicates'); setView('duplicates') }}
                >
                  <span className="ft-metric__label">
                    <CopyCheck size={12} strokeWidth={2} aria-hidden="true" />
                    <span>重复文件</span>
                  </span>
                  <strong className="ft-metric__value">
                    {summary.duplicateGroupCount} <span>组已确认</span>
                  </strong>
                  <small className="ft-metric__sub">
                    {summary.removableReclaimableBytes > 0
                      ? `最多可释放 ${formatBytes(summary.removableReclaimableBytes)}`
                      : summary.candidateGroupCount > 0
                        ? `另有 ${summary.candidateGroupCount} 组待核验`
                        : '未发现重复文件'}
                  </small>
                </button>
              </div>

              <div className="ft-summary-strip__meta">
                <div className="ft-summary-strip__tags">
                  <span className="ft-scope-summary">
                    <span>范围：</span>
                    {results.roots.map((root, index) => {
                      const name = results.mounts[root.id.scope]?.name ?? ''
                      return (
                        <span key={root.id.scope} translate="no">
                          {index > 0 ? '、' : ''}
                          {root.path === '/' ? name : `${name} (${root.path})`}
                        </span>
                      )
                    })}
                  </span>

                  {results.cachedFolders > 0 ? (
                    <span className="ft-pill ft-pill--subtle">
                      <span>已复用目录缓存</span> · {results.cachedFolders} <span>个文件夹</span>
                    </span>
                  ) : null}

                  {summary.hasIncompleteRoot ? (
                    <span className="ft-pill ft-pill--warning">
                      <TriangleAlert size={11} aria-hidden="true" />
                      <span>部分目录无法读取或受到保护，结果仅包含已扫描的内容。</span>
                    </span>
                  ) : null}
                </div>

                <button
                  className="button button--subtle ft-adjust-btn"
                  type="button"
                  disabled={work !== null}
                  onClick={() => setView('hub')}
                >
                  <SlidersHorizontal size={12} strokeWidth={1.9} aria-hidden="true" />
                  <span>调整范围</span>
                </button>
              </div>
            </section>

            {view === 'usage' ? (
              <SpaceUsageView
                results={results}
                breadcrumb={breadcrumb}
                childrenEntries={children}
                busy={work !== null}
                browsing={work === 'browse'}
                onBrowse={(next) => void browse(next)}
                canOpenInFiles={canOpenInFiles}
                onOpenInFiles={onOpenLocation}
              />
            ) : null}

            {view === 'empty' ? (
              <EmptyFoldersView
                results={results}
                busy={work !== null}
                onRemove={setRemoval}
                canOpenInFiles={canOpenInFiles}
                onOpenInFiles={onOpenLocation}
              />
            ) : null}

            {view === 'duplicates' ? (
              <DuplicatesView
                results={results}
                summary={summary}
                busy={work !== null}
                verifyingSize={verifyingSize}
                progress={progress}
                onVerify={(size) => void verify(size)}
                onCancelVerify={cancelVerification}
                onRemove={setRemoval}
                canOpenInFiles={canOpenInFiles}
                onOpenInFiles={onOpenLocation}
              />
            ) : null}
          </>
        ) : (
          <ToolEntryHub
            mounts={mounts}
            mountsLoading={mountsLoading}
            sharedUnavailable={sharedUnavailable}
            selectedMounts={selectedMounts}
            onToggleMount={toggleMount}
            onSelectAllOnline={selectAllOnline}
            onSelectPrimaryOnly={selectPrimaryOnly}
            onRefreshMounts={refreshMounts}
            canScopeToFolder={canScopeToFolder}
            currentPath={currentPath}
            onlyCurrentFolder={onlyCurrentFolder}
            onOnlyCurrentFolderChange={setOnlyCurrentFolder}
            preferredTool={preferredTool}
            onSelectPreferredTool={setPreferredTool}
            results={results}
            summary={summary}
            onOpenTool={(tool) => {
              setPreferredTool(tool)
              setView(tool)
            }}
            onScan={(refresh, targetTool) => triggerScan(refresh, targetTool)}
            busy={work !== null}
          />
        )}
      </div>

      {removal ? (
        <Modal
          title={removal.keep ? '删除重复文件？' : '删除空文件夹？'}
          actionLabel="删除"
          secondaryActionLabel="取消"
          onClose={() => setRemoval(null)}
          onSecondaryAction={() => setRemoval(null)}
          onAction={confirmRemove}
          className="vault-key-modal--danger"
        >
          <div className="ft-confirm-modal">
            <div className="ft-confirm-box ft-confirm-box--danger">
              <span className="ft-confirm-box__label">
                <Trash2 size={13} aria-hidden="true" />
                <span>即将删除</span>
              </span>
              <div className="ft-confirm-box__item">
                {removal.target.directory ? (
                  <Folder size={16} aria-hidden="true" />
                ) : (
                  <FileTypeIcon kind={fileKindByName(entryBasename(removal.target.path))} />
                )}
                <div className="ft-confirm-box__meta">
                  <strong translate="no">{mountName(removal.target)} · {removal.target.path}</strong>
                  {!removal.target.directory ? (
                    <small>{formatBytes(removal.target.size)}</small>
                  ) : null}
                </div>
              </div>
            </div>

            {removal.keep ? (
              <div className="ft-confirm-box ft-confirm-box--keep">
                <span className="ft-confirm-box__label">
                  <ShieldCheck size={13} aria-hidden="true" />
                  <span>将保留此副本</span>
                </span>
                <div className="ft-confirm-box__meta">
                  <strong translate="no">{mountName(removal.keep)} · {removal.keep.path}</strong>
                </div>
              </div>
            ) : null}

            <p className="ft-confirm-modal__note">
              删除前会再次校验文件或目录未被修改；Koofr 主存储中删除的项目会移入回收站。
            </p>
          </div>
        </Modal>
      ) : null}
    </main>
  )
}
