import { ChevronDown, FileUp, Layers3 } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'

import type { UploadMode } from '../../types/files'

interface UploadModeMenuProps {
  readonly disabled: boolean
  readonly onSelect: (mode: UploadMode) => void
}

export function UploadModeMenu({ disabled, onSelect }: UploadModeMenuProps) {
  const [open, setOpen] = useState(false)
  const anchorRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (event.target instanceof Node && !anchorRef.current?.contains(event.target)) {
        setOpen(false)
      }
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }
    document.addEventListener('pointerdown', closeOnOutsidePointer)
    document.addEventListener('keydown', closeOnEscape)
    return () => {
      document.removeEventListener('pointerdown', closeOnOutsidePointer)
      document.removeEventListener('keydown', closeOnEscape)
    }
  }, [open])

  const select = (mode: UploadMode) => {
    setOpen(false)
    onSelect(mode)
  }

  return (
    <div className="menu-anchor" ref={anchorRef}>
      <button
        className="primary-button"
        type="button"
        disabled={disabled}
        aria-label="选择上传方式"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        <FileUp size={18} />上传<ChevronDown size={16} />
      </button>
      {open ? (
        <div className="new-menu upload-mode-menu" role="menu">
          <button role="menuitem" type="button" onClick={() => select('compatible')}>
            <FileUp size={17} />上传文件
          </button>
          <button role="menuitem" type="button" onClick={() => select('split')}>
            <Layers3 size={17} />分卷上传大文件（可断点续传）
          </button>
        </div>
      ) : null}
    </div>
  )
}
