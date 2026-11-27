import {
  ArrowDownToLine,
  ChevronRight,
  Clock3,
  Cloud,
  Folder,
  FolderSync,
  ListChecks,
  HelpCircle,
  LockKeyhole,
  LogOut,
  Settings,
  Share2,
  Trash2,
} from 'lucide-react'
import { memo } from 'react'
import { formatStorageMegabytes } from '../utils/format'

export type NavView = 'files' | 'recent' | 'shared' | 'trash' | 'sync' | 'vault' | 'places' | 'fileTools'

const fileItems: ReadonlyArray<{ readonly id: NavView; readonly label: string; readonly icon: typeof Folder }> = [
  { id: 'files', label: '我的文件', icon: Folder },
  { id: 'recent', label: '最近的文件', icon: Clock3 },
  { id: 'shared', label: '共享', icon: Share2 },
  { id: 'trash', label: '回收站', icon: Trash2 },
]

const workspaceToolItems: ReadonlyArray<{ readonly id: NavView; readonly label: string; readonly icon: typeof FolderSync }> = [
  { id: 'sync', label: '同步', icon: FolderSync },
  { id: 'vault', label: '私人保险箱', icon: LockKeyhole },
  { id: 'fileTools', label: '文件工具', icon: ListChecks },
  { id: 'places', label: '存储位置', icon: Cloud },
]

interface AppSidebarProps {
  readonly activeView: NavView
  readonly onSelect: (view: NavView) => void
  readonly transferVisible: boolean
  readonly activeTransferCount: number
  readonly onToggleTransfers: () => void
  readonly syncActiveCount: number
  readonly syncIssueCount: number
  readonly mountCount: number
  readonly onSettingsClick: () => void
  readonly onLogoutClick: () => void
  readonly onHelpClick: () => void
  readonly storageName: string
  readonly storageUsed: number | null
  readonly storageTotal: number | null
}

export const AppSidebar = memo(function AppSidebar({
  activeView,
  onSelect,
  transferVisible,
  activeTransferCount,
  onToggleTransfers,
  syncActiveCount,
  syncIssueCount,
  mountCount,
  onSettingsClick,
  onLogoutClick,
  onHelpClick,
  storageName,
  storageUsed,
  storageTotal,
}: AppSidebarProps) {
  const storagePercent = storageUsed !== null && storageTotal !== null && storageTotal > 0
    ? Math.min(100, Math.max(0, (storageUsed / storageTotal) * 100))
    : 0

  return (
    <aside className="sidebar" aria-label="主导航">
      <div className="sidebar__scroll">
        <div className="sidebar__section sidebar__section--first">
          <p className="sidebar__section-label">文件</p>
          <nav className="sidebar__nav" aria-label="文件导航">
            {fileItems.map(({ id, label, icon: Icon }) => (
              <button
                className={`nav-item${activeView === id ? ' nav-item--active' : ''}`}
                key={id}
                type="button"
                title={label}
                onClick={() => onSelect(id)}
                aria-current={activeView === id ? 'page' : undefined}
              >
                <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
                <span className="nav-item__label">{label}</span>
              </button>
            ))}
          </nav>
        </div>

        <div className="sidebar__section">
          <p className="sidebar__section-label">工具与存储</p>
          <nav className="sidebar__nav" aria-label="工具与存储导航">
            {workspaceToolItems.map(({ id, label, icon: Icon }) => {
              const showSyncIssues = id === 'sync' && syncIssueCount > 0
              const showSyncActive = id === 'sync' && syncIssueCount === 0 && syncActiveCount > 0
              const showMountCount = id === 'places' && mountCount > 1
              return (
                <button
                  className={`nav-item${activeView === id ? ' nav-item--active' : ''}`}
                  key={id}
                  type="button"
                  title={label}
                  onClick={() => onSelect(id)}
                  aria-current={activeView === id ? 'page' : undefined}
                >
                  <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
                  <span className="nav-item__label">{label}</span>
                  {showSyncIssues ? (
                    <span className="nav-badge nav-badge--warning" aria-label={`${syncIssueCount} 个同步问题`}>
                      {syncIssueCount}
                    </span>
                  ) : showSyncActive ? (
                    <span className="nav-dot nav-dot--active" aria-label="正在同步" />
                  ) : showMountCount ? (
                    <span className="nav-badge nav-badge--muted">{mountCount}</span>
                  ) : null}
                </button>
              )
            })}
          </nav>
        </div>
      </div>

      <div className="sidebar__footer">
        <button
          className={`nav-item sidebar__transfer-toggle${transferVisible ? ' sidebar__transfer-toggle--open' : ''}`}
          type="button"
          title={transferVisible ? '收起传输面板' : '打开传输面板'}
          aria-label={activeTransferCount > 0 ? `传输队列，${activeTransferCount} 项进行中` : '传输队列'}
          aria-pressed={transferVisible}
          onClick={onToggleTransfers}
        >
          <ArrowDownToLine size={16} strokeWidth={1.8} aria-hidden="true" />
          <span className="nav-item__label">传输队列</span>
          {activeTransferCount > 0 ? (
            <span className="nav-badge nav-badge--accent">{activeTransferCount}</span>
          ) : null}
        </button>

        <button
          className={`storage storage--button${activeView === 'places' ? ' storage--active' : ''}`}
          type="button"
          title="查看已连接存储"
          onClick={() => onSelect('places')}
        >
          <div className="storage__head">
            <span translate={storageName ? 'no' : undefined}>{storageName || '存储空间'}</span>
            {storageTotal !== null && storageTotal > 0 ? (
              <em>{storagePercent.toFixed(0)}%</em>
            ) : (
              <ChevronRight size={13} strokeWidth={2} aria-hidden="true" />
            )}
          </div>
          <div className="storage__track" aria-label={`已使用 ${storagePercent.toFixed(1)}% 存储空间`}>
            <span style={{ width: `${storagePercent}%` }} />
          </div>
          <small>
            {storageTotal !== null && storageTotal > 0
              ? `已使用 ${formatStorageMegabytes(storageUsed)} / ${formatStorageMegabytes(storageTotal)}`
              : '容量信息不可用'}
          </small>
        </button>

        <div className="sidebar__footer-actions">
          <button className="footer-link footer-link--primary" type="button" title="设置" onClick={onSettingsClick}>
            <Settings size={15} strokeWidth={1.75} aria-hidden="true" />
            <span className="footer-link__label">设置</span>
          </button>
          <button
            className="footer-icon-button"
            type="button"
            aria-label="帮助与反馈"
            title="帮助与反馈"
            onClick={onHelpClick}
          >
            <HelpCircle size={15} strokeWidth={1.75} aria-hidden="true" />
          </button>
          <button
            className="footer-icon-button"
            type="button"
            aria-label="退出登录"
            title="退出登录"
            onClick={onLogoutClick}
          >
            <LogOut size={15} strokeWidth={1.75} aria-hidden="true" />
          </button>
        </div>
      </div>
    </aside>
  )
})
