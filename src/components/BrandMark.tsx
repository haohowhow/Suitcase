interface BrandMarkProps {
  compact?: boolean
}

export function BrandMark({ compact = false }: BrandMarkProps) {
  return (
    <div className={compact ? 'brand-mark brand-mark--compact' : 'brand-mark'}>
      <span className="brand-mark__glyph" aria-hidden="true">
        <svg viewBox="0 0 24 24" focusable="false">
          <rect className="brand-mark__bg" x="1" y="1" width="22" height="22" rx="6" />
          <rect x="1.5" y="1.5" width="21" height="21" rx="5.5" fill="none" stroke="#ffffff" strokeOpacity="0.24" />
          <path
            className="brand-mark__handle"
            d="M9.2 8.2V6.8a1.6 1.6 0 0 1 1.6-1.6h2.4a1.6 1.6 0 0 1 1.6 1.6v1.4"
          />
          <rect className="brand-mark__body" x="5.2" y="8" width="13.6" height="10.2" rx="2.3" />
          <path className="brand-mark__clasp" d="M5.2 12.1h13.6M10.6 12.1v1.7h2.8v-1.7" />
        </svg>
      </span>
      <strong translate="no">Koofr</strong>
    </div>
  )
}
