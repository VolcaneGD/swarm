export type LayoutPreferences = {
  version: 1
  treeWidth: number
  composerHeight: number
  logExpanded?: boolean
}

export const DEFAULT_LAYOUT_PREFERENCES: LayoutPreferences = {
  version: 1,
  treeWidth: 280,
  composerHeight: 230,
}

const TREE_WIDTH_MIN = 220
const TREE_WIDTH_MAX = 480
export const COMPOSER_HEIGHT_MIN = 200
const COMPOSER_HEIGHT_MAX = 480

function clamp(value: unknown, fallback: number, minimum: number, maximum: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback
  return Math.max(minimum, Math.min(maximum, value))
}

export function normalizeLayoutPreferences(value: unknown): LayoutPreferences {
  if (typeof value !== 'object' || value === null || !('version' in value) || value.version !== 1) {
    return { ...DEFAULT_LAYOUT_PREFERENCES }
  }
  const preferences = value as Record<string, unknown>
  return {
    version: 1,
    treeWidth: clamp(preferences.treeWidth, DEFAULT_LAYOUT_PREFERENCES.treeWidth, TREE_WIDTH_MIN, TREE_WIDTH_MAX),
    composerHeight: clamp(preferences.composerHeight, DEFAULT_LAYOUT_PREFERENCES.composerHeight, COMPOSER_HEIGHT_MIN, COMPOSER_HEIGHT_MAX),
    ...(preferences.logExpanded === true ? { logExpanded: true } : {}),
  }
}

export function parseLayoutPreferences(serialized: string | null): LayoutPreferences {
  if (serialized === null) return { ...DEFAULT_LAYOUT_PREFERENCES }
  try {
    return normalizeLayoutPreferences(JSON.parse(serialized))
  } catch {
    return { ...DEFAULT_LAYOUT_PREFERENCES }
  }
}

export function layoutPreferencesKey(accountId: string): string {
  const isDesktop = typeof window !== 'undefined'
    && (window.location.protocol === 'tauri:' || window.location.hostname === 'tauri.localhost')
  const scope = isDesktop ? 'desktop' : 'preview'
  return `swarm:layout:v1:${scope}:${encodeURIComponent(accountId || 'test')}`
}

export function readLayoutPreferences(accountId: string): LayoutPreferences {
  return parseLayoutPreferences(window.localStorage.getItem(layoutPreferencesKey(accountId)))
}

export function writeLayoutPreferences(accountId: string, preferences: LayoutPreferences): void {
  const normalized = normalizeLayoutPreferences(preferences)
  window.localStorage.setItem(layoutPreferencesKey(accountId), JSON.stringify(normalized))
}
