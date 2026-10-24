// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { guiPreferenceGet, guiPreferenceSet } from './bridge'
import { isDesktopPreferenceHost, loadUiPreference, saveUiPreference } from './uiPreferenceStorage'

vi.mock('./bridge', () => ({ guiPreferenceGet: vi.fn(), guiPreferenceSet: vi.fn() }))

const key = 'legacy-layout'
const normalize = (value: unknown) => {
  const item = value as { treeWidth?: unknown; composerHeight?: unknown }
  return {
    version: 1,
    treeWidth: typeof item?.treeWidth === 'number' ? item.treeWidth : 280,
    composerHeight: typeof item?.composerHeight === 'number' ? item.composerHeight : 230,
  }
}
const parse = (value: string | null) => value === null ? normalize(null) : normalize(JSON.parse(value))
const defaults = normalize(null)
const legacyValue = { version: 1, treeWidth: 360, composerHeight: 320 }
const load = (desktop = true) => loadUiPreference({
  accountId: 'account-a', scope: 'layout' as const, legacyStorageKey: key,
  defaultValue: defaults, normalize, parseLegacy: parse, desktop,
})

describe('desktop UI preference storage', () => {
  beforeEach(() => {
    window.localStorage.clear()
    vi.mocked(guiPreferenceGet).mockReset()
    vi.mocked(guiPreferenceSet).mockReset()
  })
  afterEach(() => window.localStorage.clear())

  it('recognizes an HTTP Tauri development host through its native internals marker', () => {
    expect(isDesktopPreferenceHost(new URL('http://127.0.0.1:5173') as unknown as Location, true)).toBe(true)
    expect(isDesktopPreferenceHost(new URL('http://127.0.0.1:5173') as unknown as Location, false)).toBe(false)
  })

  it('uses the Service DB value over stale renderer cache', async () => {
    window.localStorage.setItem(key, JSON.stringify(legacyValue))
    vi.mocked(guiPreferenceGet).mockResolvedValue({ value: { version: 1, treeWidth: 420, composerHeight: 400 }, migration_allowed: true })

    const result = await load()

    expect(result).toMatchObject({ value: { version: 1, treeWidth: 420, composerHeight: 400 }, source: 'database', error: null })
    expect(guiPreferenceSet).not.toHaveBeenCalled()
    expect(window.localStorage.getItem(key)).toBeNull()
  })

  it('never imports stale renderer cache when the DB disallows migration or omits the flag', async () => {
    window.localStorage.setItem(key, JSON.stringify(legacyValue))
    vi.mocked(guiPreferenceGet).mockResolvedValue({ value: null, migration_allowed: false })
    expect(await load()).toMatchObject({ value: defaults, source: 'default', error: null })
    expect(guiPreferenceSet).not.toHaveBeenCalled()
    expect(window.localStorage.getItem(key)).toBeNull()

    window.localStorage.setItem(key, JSON.stringify(legacyValue))
    vi.mocked(guiPreferenceGet).mockResolvedValue({ value: null } as never)
    expect(await load()).toMatchObject({ value: defaults, source: 'default', error: null })
    expect(guiPreferenceSet).not.toHaveBeenCalled()
  })

  it('migrates a legacy desktop value only when the DB explicitly permits it', async () => {
    window.localStorage.setItem(key, JSON.stringify(legacyValue))
    vi.mocked(guiPreferenceGet).mockResolvedValue({ value: null, migration_allowed: true })
    vi.mocked(guiPreferenceSet).mockResolvedValue({ saved: true })

    const result = await load()

    expect(guiPreferenceSet).toHaveBeenCalledWith('account-a', { scope: 'layout', value: legacyValue })
    expect(result).toMatchObject({ value: legacyValue, source: 'legacy', error: null })
    expect(window.localStorage.getItem(key)).toBeNull()
  })

  it('keeps legacy data and the in-memory value visible when migration save fails', async () => {
    window.localStorage.setItem(key, JSON.stringify(legacyValue))
    vi.mocked(guiPreferenceGet).mockResolvedValue({ value: null, migration_allowed: true })
    vi.mocked(guiPreferenceSet).mockRejectedValue(new Error('database unavailable'))

    const result = await load()

    expect(result).toMatchObject({ value: legacyValue, source: 'legacy', error: 'write' })
    expect(window.localStorage.getItem(key)).toBe(JSON.stringify(legacyValue))
  })

  it('does not fall back to stale cache after a DB read failure and leaves preview storage local', async () => {
    window.localStorage.setItem(key, JSON.stringify(legacyValue))
    vi.mocked(guiPreferenceGet).mockRejectedValue(new Error('database unavailable'))
    expect(await load()).toMatchObject({ value: defaults, source: 'default', error: 'read' })
    expect(window.localStorage.getItem(key)).toBe(JSON.stringify(legacyValue))

    await saveUiPreference({ accountId: 'account-a', scope: 'layout', legacyStorageKey: key, value: legacyValue, normalize, desktop: false })
    expect(window.localStorage.getItem(key)).toBe(JSON.stringify(legacyValue))
    expect(guiPreferenceSet).not.toHaveBeenCalled()
  })
})
