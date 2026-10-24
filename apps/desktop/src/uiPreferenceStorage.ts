import { guiPreferenceGet, guiPreferenceSet } from './bridge'

export type UiPreferenceScope = 'layout' | 'conversation_view' | 'thread_scroll'

export type UiPreferenceLoadResult<T> = {
  value: T
  source: 'database' | 'legacy' | 'default' | 'preview'
  error: 'read' | 'write' | null
}

type LoadUiPreferenceOptions<T> = {
  accountId: string
  scope: UiPreferenceScope
  key?: string
  legacyStorageKey: string
  defaultValue: T
  normalize: (value: unknown) => T
  parseLegacy: (serialized: string | null) => T
  desktop?: boolean
  isCurrent?: () => boolean
}

export function isDesktopPreferenceHost(location = typeof window === 'undefined' ? null : window.location, hasTauriInternals = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window): boolean {
  return hasTauriInternals || (location !== null && (location.protocol === 'tauri:' || location.hostname === 'tauri.localhost'))
}

export async function loadUiPreference<T>(options: LoadUiPreferenceOptions<T>): Promise<UiPreferenceLoadResult<T>> {
  const desktop = options.desktop ?? isDesktopPreferenceHost()
  if (!desktop) {
    try {
      const raw = window.localStorage.getItem(options.legacyStorageKey)
      return { value: options.parseLegacy(raw), source: 'preview', error: null }
    } catch {
      return { value: options.defaultValue, source: 'default', error: 'read' }
    }
  }

  let persisted: Awaited<ReturnType<typeof guiPreferenceGet>>
  try {
    persisted = await guiPreferenceGet(options.accountId, { scope: options.scope, ...(options.key ? { key: options.key } : {}) })
  } catch {
    return { value: options.defaultValue, source: 'default', error: 'read' }
  }
  if (options.isCurrent && !options.isCurrent()) return { value: options.defaultValue, source: 'default', error: null }

  if (persisted.value !== null) {
    try { window.localStorage.removeItem(options.legacyStorageKey) } catch { /* The database remains authoritative. */ }
    return { value: options.normalize(persisted.value), source: 'database', error: null }
  }

  if (persisted.migration_allowed !== true) {
    try { window.localStorage.removeItem(options.legacyStorageKey) } catch { /* Never import legacy desktop state after restore. */ }
    return { value: options.defaultValue, source: 'default', error: null }
  }

  let legacy: string | null
  try {
    legacy = window.localStorage.getItem(options.legacyStorageKey)
  } catch {
    return { value: options.defaultValue, source: 'default', error: 'read' }
  }
  if (legacy === null) return { value: options.defaultValue, source: 'default', error: null }

  const value = options.parseLegacy(legacy)
  if (options.isCurrent && !options.isCurrent()) return { value: options.defaultValue, source: 'default', error: null }
  try {
    const result = await guiPreferenceSet(options.accountId, {
      scope: options.scope,
      ...(options.key ? { key: options.key } : {}),
      value,
    })
    if (!result.saved) throw new Error('Preference migration was not acknowledged.')
    try { window.localStorage.removeItem(options.legacyStorageKey) } catch { /* A later database read still wins. */ }
    return { value, source: 'legacy', error: null }
  } catch {
    return { value, source: 'legacy', error: 'write' }
  }
}

type SaveUiPreferenceOptions<T> = {
  accountId: string
  scope: UiPreferenceScope
  key?: string
  legacyStorageKey: string
  value: T
  normalize: (value: unknown) => T
  desktop?: boolean
}

export async function saveUiPreference<T>(options: SaveUiPreferenceOptions<T>): Promise<void> {
  const value = options.normalize(options.value)
  const desktop = options.desktop ?? isDesktopPreferenceHost()
  if (desktop) {
    const result = await guiPreferenceSet(options.accountId, {
      scope: options.scope,
      ...(options.key ? { key: options.key } : {}),
      value,
    })
    if (!result.saved) throw new Error('Preference write was not acknowledged.')
    return
  }
  window.localStorage.setItem(options.legacyStorageKey, options.scope === 'conversation_view' ? String(value) : JSON.stringify(value))
}
