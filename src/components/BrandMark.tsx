import { useId } from 'react'

interface BrandGlyphProps {
  className?: string
}

export function BrandGlyph({ className = 'brand-mark__glyph' }: BrandGlyphProps) {
  const gradientId = useId()

  return (
    <span className={className} aria-hidden="true">
      <svg viewBox="0 0 64 64" focusable="false">
        <defs>
          <linearGradient id={gradientId} x1="0" y1="0" x2="0.4" y2="1">
            <stop className="brand-mark__bg-start" offset="0%" />
            <stop className="brand-mark__bg-end" offset="100%" />
          </linearGradient>
        </defs>
        <rect x="2" y="2" width="60" height="60" rx="15" fill={`url(#${gradientId})`} />
        <rect
          className="brand-mark__rim"
          x="2.75"
          y="2.75"
          width="58.5"
          height="58.5"
          rx="14.25"
        />
        <path
          className="brand-mark__handle"
          d="M24 22v-3.5a4.5 4.5 0 0 1 4.5-4.5h7a4.5 4.5 0 0 1 4.5 4.5V22"
        />
        <rect className="brand-mark__body" x="13" y="21" width="38" height="28" rx="7" />
        <path
          className="brand-mark__lid"
          d="M13 28a7 7 0 0 1 7-7h24a7 7 0 0 1 7 7v4.2a6 6 0 0 1-5.15 5.94L35.2 39.6a22 22 0 0 1-6.4 0l-10.65-1.46A6 6 0 0 1 13 32.2V28Z"
        />
        <rect className="brand-mark__clasp" x="27.5" y="33.5" width="9" height="5.5" rx="2.75" />
      </svg>
    </span>
  )
}

interface BrandMarkProps {
  compact?: boolean
}

export function BrandMark({ compact = false }: BrandMarkProps) {
  return (
    <div className={compact ? 'brand-mark brand-mark--compact' : 'brand-mark'}>
      <BrandGlyph />
      <strong translate="no">Suitcase</strong>
    </div>
  )
}
