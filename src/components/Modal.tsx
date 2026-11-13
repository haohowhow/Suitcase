import { X } from 'lucide-react'
import { useEffect, useRef } from 'react'
import type { ReactNode } from 'react'

interface ModalProps {
  readonly title: string
  readonly children: ReactNode
  readonly actionLabel: string
  readonly onClose: () => void
  readonly onAction?: () => void
  readonly actionDisabled?: boolean
  readonly secondaryActionLabel?: string
  readonly onSecondaryAction?: () => void
  readonly secondaryActionDisabled?: boolean
  readonly wide?: boolean
  readonly extraWide?: boolean
  /** Extra class on the dialog surface, for dialogs that need their own layout. */
  readonly className?: string
}

export function Modal({
  title,
  children,
  actionLabel,
  onClose,
  onAction,
  actionDisabled = false,
  secondaryActionLabel,
  onSecondaryAction,
  secondaryActionDisabled = false,
  wide = false,
  extraWide = false,
  className,
}: ModalProps) {
  const dialogRef = useRef<HTMLDialogElement>(null)
  const closeButtonRef = useRef<HTMLButtonElement>(null)
  const onCloseRef = useRef(onClose)

  useEffect(() => {
    onCloseRef.current = onClose
  }, [onClose])

  useEffect(() => {
    const previousFocus = document.activeElement
    const dialog = dialogRef.current
    if (dialog && typeof dialog.showModal === 'function') dialog.showModal()
    else dialog?.setAttribute('open', '')
    closeButtonRef.current?.focus({ preventScroll: true })
    return () => {
      if (dialog?.open && typeof dialog.close === 'function') dialog.close()
      if (previousFocus instanceof HTMLElement) previousFocus.focus()
    }
  }, [])

  return (
    <dialog
      ref={dialogRef}
      className="modal-backdrop"
      aria-labelledby="modal-title"
      onCancel={(event) => {
        event.preventDefault()
        onCloseRef.current()
      }}
    >
      <button
        className="modal-dismiss-surface"
        type="button"
        aria-hidden="true"
        tabIndex={-1}
        onClick={() => onCloseRef.current()}
      />
      <section
        className={`modal${wide ? ' modal--wide' : ''}${extraWide ? ' modal--extra-wide' : ''}${className ? ` ${className}` : ''}`}
      >
        <button ref={closeButtonRef} className="icon-button modal__close" type="button" aria-label="关闭" onClick={onClose}>
          <X size={18} />
        </button>
        <h2 id="modal-title">{title}</h2>
        <div className="modal__content">{children}</div>
        <div className="modal__actions">
          {secondaryActionLabel ? (
            <button
              className="secondary-button"
              type="button"
              disabled={secondaryActionDisabled}
              onClick={onSecondaryAction ?? onClose}
            >
              {secondaryActionLabel}
            </button>
          ) : null}
          <button
            className="primary-button"
            type="button"
            disabled={actionDisabled}
            onClick={onAction ?? onClose}
          >
            {actionLabel}
          </button>
        </div>
      </section>
    </dialog>
  )
}
