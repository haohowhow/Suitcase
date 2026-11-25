import {
  Clock3,
  Cloud,
  Folder,
  FolderSync,
  HelpCircle,
  LockKeyhole,
  LogOut,
  Palette,
  Settings,
  Share,
  Share2,
  Trash2,
} from 'lucide-react'
import { memo } from 'react'
import { formatStorageMegabytes } from '../utils/format'

const primaryItems = [
  { label: '我的文件', icon: Folder },
  { label: '最近的文件', icon: Clock3 },
  { label: '已共享', icon: Share2 },
  { label: '回收站', icon: Trash2 },
  { label: '同步', icon: FolderSync },
]

interface AppSidebarProps {
  activeItem: string
  onSelect: (label: string) => void
  onSettingsClick: () => void
  onThemeClick: () => void
  onVaultClick: () => void
  onLinksClick: () => void
  onPlacesClick: () => void
  onLogoutClick: () => void
  onHelpClick: () => void
  storageName: string
  storageUsed: number | null
  storageTotal: number | null
}

export const AppSidebar = memo(function AppSidebar({
  activeItem,
  onSelect,
  onSettingsClick,
  onThemeClick,
  onVaultClick,
  onLinksClick,
  onPlacesClick,
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
      <nav className="sidebar__nav">
        {primaryItems.map(({ label, icon: Icon }) => (
          <button
            className={`nav-item${activeItem === label ? ' nav-item--active' : ''}`}
            key={label}
            type="button"
            onClick={() => onSelect(label)}
            aria-current={activeItem === label ? 'page' : undefined}
          >
            <Icon size={20} strokeWidth={1.9} />
            <span>{label}</span>
          </button>
        ))}
      </nav>

      <div className="sidebar__section">
        <p className="sidebar__section-label">保险箱</p>
        <button className="nav-item nav-item--vault" type="button" onClick={onVaultClick}>
          <LockKeyhole size={19} strokeWidth={1.9} />
          <span>私人保险箱</span>
        </button>
      </div>

      <div className="sidebar__section sidebar__section--account">
        <p className="sidebar__section-label">账户功能</p>
        <button className="nav-item" type="button" onClick={onLinksClick}>
          <Share size={19} strokeWidth={1.9} />
          <span>分享链接</span>
        </button>
        <button className="nav-item" type="button" onClick={onPlacesClick}>
          <Cloud size={19} strokeWidth={1.9} />
          <span>已连接存储</span>
        </button>
      </div>

      <div className="sidebar__footer">
        <div className="storage">
          <span translate={storageName ? 'no' : undefined}>{storageName || '存储空间'}</span>
          <small>
            {storageTotal !== null && storageTotal > 0
              ? `已使用 ${formatStorageMegabytes(storageUsed)} / ${formatStorageMegabytes(storageTotal)}`
              : '容量信息不可用'}
          </small>
          <div className="storage__track" aria-label={`已使用 ${storagePercent.toFixed(1)}% 存储空间`}>
            <span style={{ width: `${storagePercent}%` }} />
          </div>
        </div>
        <button className="footer-link" type="button" onClick={onThemeClick}>
          <Palette size={17} />
          外观主题
        </button>
        <button className="footer-link" type="button" onClick={onSettingsClick}>
          <Settings size={17} />
          设置
        </button>
        <button className="footer-link" type="button" onClick={onLogoutClick}>
          <LogOut size={17} />
          退出登录
        </button>
        <button className="footer-link" type="button" onClick={onHelpClick}>
          <HelpCircle size={17} />
          帮助与反馈
        </button>
      </div>
    </aside>
  )
})
