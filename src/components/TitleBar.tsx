import { memo } from 'react'
import { koofr } from '../services/koofr'
import { BrandMark } from './BrandMark'

export const TitleBar = memo(function TitleBar() {
  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar__drag-region" data-tauri-drag-region>
        <BrandMark compact />
      </div>
      <div className="window-controls" aria-label="窗口控件">
        <button type="button" aria-label="最小化窗口" title="最小化" onClick={() => koofr.runWindowCommand('minimize')}>
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" focusable="false">
            <path d="M1 5h8" stroke="currentColor" strokeWidth="1" />
          </svg>
        </button>
        <button type="button" aria-label="最大化或还原窗口" title="最大化或还原" onClick={() => koofr.runWindowCommand('toggleMaximize')}>
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" focusable="false">
            <rect x="1.5" y="1.5" width="7" height="7" rx="0.5" fill="none" stroke="currentColor" strokeWidth="1" />
          </svg>
        </button>
        <button className="window-control--close" type="button" aria-label="关闭窗口" title="关闭" onClick={() => koofr.runWindowCommand('close')}>
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" focusable="false">
            <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" strokeWidth="1.1" />
          </svg>
        </button>
      </div>
    </header>
  )
})
