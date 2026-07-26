import { ChevronRight, Folder, FolderInput, RefreshCw } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Modal } from '../../components/Modal'
import { commandErrorMessage, koofr } from '../../services/koofr'
import type { KoofrMount, RemoteFile } from '../../types/backend'
import { isDirectory, pathCrumbs } from './filePresentation'

interface MoveDestinationDialogProps {
  readonly files: RemoteFile[]
  readonly mountId: string
  readonly mounts: KoofrMount[]
  readonly busy: boolean
  readonly onClose: () => void
  readonly onConfirm: (destinationMountId: string, destinationDirectory: string) => void
}

export function MoveDestinationDialog({
  files,
  mountId,
  mounts,
  busy,
  onClose,
  onConfirm,
}: MoveDestinationDialogProps) {
  const [currentMountId, setCurrentMountId] = useState(mountId)
  const [currentPath, setCurrentPath] = useState('/')
  const [folders, setFolders] = useState<RemoteFile[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const requestSequence = useRef(0)

  const loadDirectory = useCallback(async (mntId: string, path: string) => {
    const requestId = ++requestSequence.current
    setLoading(true)
    setError('')
    try {
      const entries = await koofr.listFiles(mntId, path, false)
      if (requestId !== requestSequence.current) return
      const dirs = entries
        .filter(isDirectory)
        .sort((left, right) =>
          left.name.localeCompare(right.name, 'zh-CN', { numeric: true }),
        )
      setFolders(dirs)
    } catch (err) {
      if (requestId !== requestSequence.current) return
      setError(commandErrorMessage(err, '无法读取文件夹内容。'))
    } finally {
      if (requestId === requestSequence.current) setLoading(false)
    }
  }, [])

  useEffect(() => {
    loadDirectory(mountId, '/')
  }, [mountId, loadDirectory])

  const navigateMount = (mntId: string) => {
    setCurrentMountId(mntId)
    setCurrentPath('/')
    loadDirectory(mntId, '/')
  }

  const navigate = (path: string) => {
    setCurrentPath(path)
    loadDirectory(currentMountId, path)
  }

  const crumbs = pathCrumbs(currentPath)

  const namesSummary = useMemo(() => {
    if (files.length === 1) return files[0].name
    return (
      files
        .map((f) => f.name)
        .slice(0, 3)
        .join('、') +
      (files.length > 3 ? ` 等 ${files.length} 个项目` : '')
    )
  }, [files])

  const parentDir = useMemo(() => {
    if (files.length === 0) return '/'
    const segments = files[0].path.split('/').filter(Boolean)
    return segments.length === 0 ? '/' : `/${segments.slice(0, -1).join('/')}`
  }, [files])

  const isSameLocation = currentMountId === mountId && currentPath === parentDir

  const currentMountName = mounts.find((m) => m.id === currentMountId)?.name ?? currentMountId

  return (
    <Modal
      title="移动到…"
      actionLabel={busy ? '正在移动…' : '移动到此处'}
      actionDisabled={busy || isSameLocation || loading}
      onClose={onClose}
      onAction={() => onConfirm(currentMountId, currentPath)}
    >
      <div className="move-dialog__summary">
        <span className="move-dialog__icon">
          <FolderInput aria-hidden="true" />
        </span>
        <span>
          <small>移动目标</small>
          <strong title={namesSummary}>{namesSummary}</strong>
        </span>
      </div>

      <div className="move-dialog__browser">
        <div className="move-dialog__nav">
          <select
            className="mount-select"
            value={currentMountId}
            disabled={loading || busy}
            aria-label="目标存储位置"
            onChange={(event) => navigateMount(event.target.value)}
          >
            {mounts.map((mount) => (
              <option value={mount.id} key={mount.id}>{mount.name}</option>
            ))}
          </select>

          <span className="move-dialog__breadcrumbs">
            <span>/</span>
            <button
              type="button"
              disabled={currentPath === '/' || loading || busy}
              onClick={() => navigate('/')}
            >
              根目录
            </button>
            {crumbs.map((crumb) => (
              <span key={crumb.path}>
                <span>/</span>
                <button
                  type="button"
                  disabled={crumb.path === currentPath || loading || busy}
                  onClick={() => navigate(crumb.path)}
                >
                  {crumb.label}
                </button>
              </span>
            ))}
          </span>
        </div>

        <div className="move-dialog__current">
          移动至 <strong>{currentMountName} / {crumbs.length > 0 ? crumbs.map((c) => c.label).join(' / ') : '根目录'}</strong>
        </div>

        {loading ? (
          <div className="move-dialog__status">
            <RefreshCw className="icon-spin" size={20} />
            <span>正在加载…</span>
          </div>
        ) : error ? (
          <div className="move-dialog__status move-dialog__status--error">
            <span>{error}</span>
            <button
              className="secondary-button"
              type="button"
              onClick={() => loadDirectory(currentMountId, currentPath)}
            >
              重试
            </button>
          </div>
        ) : folders.length === 0 ? (
          <div className="move-dialog__status move-dialog__status--muted">
            <span>此文件夹没有子文件夹</span>
          </div>
        ) : (
          <div className="move-dialog__folder-list">
            {folders.map((folder) => (
              <button
                key={folder.path}
                type="button"
                className="move-dialog__folder-row"
                disabled={busy}
                onClick={() => navigate(folder.path)}
                title={folder.name}
              >
                <Folder size={18} />
                <span>{folder.name}</span>
                <ChevronRight size={16} />
              </button>
            ))}
          </div>
        )}
      </div>
    </Modal>
  )
}
