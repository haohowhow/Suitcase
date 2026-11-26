import { ArrowRight, Cloud, Copy, Database, ExternalLink, HardDrive, RefreshCw } from 'lucide-react'
import { useMemo, useState } from 'react'
import { formatStorageMegabytes } from '../../utils/format'
import type { KoofrMount } from '../../types/backend'
import { identifyMountProvider } from './mountProviders'

const ACCOUNT_URL = 'https://app.koofr.net'

interface ConnectedPlacesWorkspaceProps {
  readonly mounts: readonly KoofrMount[]
  readonly activeMountId: string
  readonly loading: boolean
  readonly onOpenMount: (mountId: string) => void
  readonly onRefresh: () => void
}

export function ConnectedPlacesWorkspace({
  mounts,
  activeMountId,
  loading,
  onOpenMount,
  onRefresh,
}: ConnectedPlacesWorkspaceProps) {
  const [copied, setCopied] = useState(false)
  const connected = useMemo(() => mounts.map((mount) => ({
    mount,
    provider: identifyMountProvider(mount),
  })), [mounts])

  const copyAccountAddress = async () => {
    try {
      await navigator.clipboard.writeText(ACCOUNT_URL)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }

  return (
    <main className="workspace places-workspace" aria-label="存储位置">
      <div className="workspace__header">
        <div>
          <h1>存储位置</h1>
          <p className="workspace__description">
            管理 Koofr 账户下所有已授权的云端存储，点击任意在线位置即可直接浏览文件。
          </p>
        </div>
        <div className="top-actions">
          <button
            className="icon-button icon-button--bordered"
            type="button"
            aria-label="刷新存储位置"
            title="刷新"
            disabled={loading}
            onClick={onRefresh}
          >
            <RefreshCw className={loading ? 'icon-spin' : ''} size={15} strokeWidth={1.8} />
          </button>
        </div>
      </div>

      <div className="places-workspace__body">
        {connected.length === 0 ? (
          <div className="empty-state">
            <Cloud size={22} strokeWidth={1.7} />
            <h2>没有可用的存储位置</h2>
            <p>刷新以重新读取当前 Koofr 账户的存储挂载点。</p>
            <button className="secondary-button" type="button" onClick={onRefresh}>刷新</button>
          </div>
        ) : (
          <div className="places-grid places-grid--workspace" aria-label="已连接存储列表">
            {connected.map(({ mount, provider }) => {
              const hasQuota = mount.spaceTotal > 0
              const percent = hasQuota
                ? Math.min(100, Math.max(0, (mount.spaceUsed / mount.spaceTotal) * 100))
                : 0
              const isCurrent = mount.id === activeMountId
              return (
                <article
                  className={`place-card place-card--workspace${isCurrent ? ' place-card--active' : ''}`}
                  data-provider={provider.id}
                  key={mount.id}
                >
                  <div className="place-card__top">
                    <span className="place-card__icon" aria-hidden="true">
                      {provider.id === 'koofr'
                        ? <Database size={17} strokeWidth={1.8} />
                        : provider.id === 'other'
                          ? <HardDrive size={17} strokeWidth={1.8} />
                          : <Cloud size={17} strokeWidth={1.8} />}
                    </span>
                    <div className="place-card__title">
                      <div className="place-card__name-row">
                        <strong translate={mount.name ? 'no' : undefined}>
                          {mount.name || provider.label}
                        </strong>
                        {mount.isPrimary ? <span className="place-card__pill">主存储</span> : null}
                        {isCurrent ? <span className="place-card__pill place-card__pill--current">当前</span> : null}
                      </div>
                      <small>{provider.label}</small>
                    </div>
                    <span className={`place-card__status${mount.online ? ' place-card__status--online' : ''}`}>
                      <span className="place-card__dot" aria-hidden="true" />
                      {mount.online ? '在线' : '暂时离线'}
                    </span>
                  </div>

                  <div className="place-card__quota">
                    {hasQuota ? (
                      <>
                        <div className="storage__track" aria-label={`已使用 ${percent.toFixed(1)}% 存储空间`}>
                          <span style={{ width: `${percent}%` }} />
                        </div>
                        <div className="place-card__quota-meta">
                          <span>已使用 {formatStorageMegabytes(mount.spaceUsed)} / {formatStorageMegabytes(mount.spaceTotal)}</span>
                          <em>{percent.toFixed(0)}%</em>
                        </div>
                      </>
                    ) : (
                      <small className="place-card__quota-managed">容量由服务提供方管理</small>
                    )}
                  </div>

                  <div className="place-card__footer">
                    <button
                      className={isCurrent ? 'secondary-button' : 'primary-button'}
                      type="button"
                      disabled={!mount.online}
                      onClick={() => onOpenMount(mount.id)}
                    >
                      浏览文件
                      <ArrowRight size={14} strokeWidth={1.85} aria-hidden="true" />
                    </button>
                  </div>
                </article>
              )
            })}
          </div>
        )}

        <section className="places-management">
          <ExternalLink aria-hidden="true" />
          <span>
            <strong>连接、移除或重新授权外部云盘</strong>
            <small>第三方存储授权目前只能在 Koofr 网页版中管理。登录网页版后打开“账户 → 位置”即可操作。</small>
          </span>
          <label>
            Koofr 网页地址
            <input value={ACCOUNT_URL} readOnly onFocus={(event) => event.currentTarget.select()} />
          </label>
          <button className="secondary-button" type="button" onClick={() => void copyAccountAddress()}>
            <Copy aria-hidden="true" />
            {copied ? '已复制' : '复制地址'}
          </button>
        </section>
      </div>
    </main>
  )
}
