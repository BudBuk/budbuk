import { useEffect, type ReactNode } from 'react'
import { CloseIcon } from './icons'

interface Props {
  title: ReactNode
  subtitle?: ReactNode
  onClose: () => void
  children: ReactNode
  width?: number
}

// A centered dialog on a dimmed overlay. Clicking the overlay or the × closes
// it, as does pressing Escape.
export default function Modal({ title, subtitle, onClose, children, width = 480 }: Props) {
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    document.body.style.overflow = 'hidden'
    return () => {
      window.removeEventListener('keydown', onKey)
      document.body.style.overflow = ''
    }
  }, [onClose])

  return (
    <div className="modal-overlay" onClick={onClose} role="presentation">
      <div
        className="modal"
        style={{ maxWidth: width }}
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <div className="modal-head-main">{title}</div>
          <button
            type="button"
            className="icon-btn modal-close"
            onClick={onClose}
            aria-label="Close"
          >
            <CloseIcon size={18} />
          </button>
        </div>
        {subtitle && <div className="modal-subtitle">{subtitle}</div>}
        <div className="modal-body">{children}</div>
      </div>
    </div>
  )
}
