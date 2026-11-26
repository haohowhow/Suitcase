import { memo } from 'react'

export interface WorkspaceTabItem<T extends string = string> {
  readonly id: T
  readonly label: string
  readonly badge?: number
}

interface WorkspaceTabsProps<T extends string> {
  readonly ariaLabel: string
  readonly items: readonly WorkspaceTabItem<T>[]
  readonly activeId: T
  readonly onChange: (id: T) => void
}

function WorkspaceTabsInner<T extends string>({
  ariaLabel,
  items,
  activeId,
  onChange,
}: WorkspaceTabsProps<T>) {
  return (
    <div className="workspace-tabs" role="tablist" aria-label={ariaLabel}>
      {items.map((item) => {
        const active = item.id === activeId
        return (
          <button
            key={item.id}
            className={`workspace-tabs__item${active ? ' workspace-tabs__item--active' : ''}`}
            type="button"
            role="tab"
            aria-selected={active}
            onClick={() => onChange(item.id)}
          >
            <span>{item.label}</span>
            {item.badge !== undefined && item.badge > 0 ? (
              <span className="workspace-tabs__badge">{item.badge}</span>
            ) : null}
          </button>
        )
      })}
    </div>
  )
}

export const WorkspaceTabs = memo(WorkspaceTabsInner) as typeof WorkspaceTabsInner
