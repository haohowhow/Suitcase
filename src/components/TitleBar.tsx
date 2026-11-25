import { Minus, Square, X } from 'lucide-react'
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
          <Minus size={16} />
        </button>
        <button type="button" aria-label="最大化或还原窗口" title="最大化或还原" onClick={() => koofr.runWindowCommand('toggleMaximize')}>
          <Square size={13} />
        </button>
        <button className="window-control--close" type="button" aria-label="关闭窗口" title="关闭" onClick={() => koofr.runWindowCommand('close')}>
          <X size={16} />
        </button>
      </div>
    </header>
  )
})
