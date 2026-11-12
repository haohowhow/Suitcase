import { useEffect, useRef } from 'react'

/**
 * Returns a ref that always holds the latest value. Long-lived effects and event
 * handlers read through it instead of listing the value as a dependency, so a new
 * callback identity does not tear them down and start them again.
 */
export function useLatestRef<T>(value: T) {
  const ref = useRef(value)
  useEffect(() => {
    ref.current = value
  })
  return ref
}
