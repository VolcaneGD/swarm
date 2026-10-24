import { useCallback, useEffect, useState } from 'react'
import { getCurrentWindow, type Theme } from '@tauri-apps/api/window'

let pending: Promise<void> = Promise.resolve()

function hasNativeWindow(): boolean {
  if (typeof window === 'undefined') return false
  const internals = (window as Window & { __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: unknown } } } }).__TAURI_INTERNALS__
  // mockIPC in the browser preview has no native window metadata.
  return typeof internals?.metadata?.currentWindow?.label === 'string'
}

export function applyNativeWindowTheme(theme: Theme, desktop = hasNativeWindow()): Promise<void> {
  if (!desktop) return Promise.resolve()
  // Preserve request order even when a previous native operation failed.
  const applied = pending.catch(() => undefined).then(() => getCurrentWindow().setTheme(theme))
  pending = applied
  return applied
}

export default function useNativeWindowTheme(theme: Theme, ready: boolean) {
  const [failed, setFailed] = useState(false)
  const [attempt, setAttempt] = useState(0)
  const retry = useCallback(() => setAttempt(value => value + 1), [])
  useEffect(() => {
    if (!ready) return
    let current = true
    void applyNativeWindowTheme(theme).then(
      () => { if (current) setFailed(false) },
      () => { if (current) setFailed(true) },
    )
    return () => { current = false }
  }, [theme, ready, attempt])
  return { failed, retry }
}
