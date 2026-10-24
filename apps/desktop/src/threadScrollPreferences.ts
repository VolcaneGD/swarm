export type ThreadScrollPreference = {
  version: 1
  postNo: number | null
  offset: number
  scrollTop: number
  atBottom: boolean
  page?: number
  pageSize?: 50 | 100 | 500 | 1000
}

export const DEFAULT_THREAD_SCROLL_PREFERENCE: ThreadScrollPreference = {
  version: 1,
  postNo: null,
  offset: 0,
  scrollTop: 0,
  atBottom: false,
  page: 1,
  pageSize: 50,
}

function finiteRange(value: unknown, fallback: number, minimum: number, maximum: number): number {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.max(minimum, Math.min(maximum, value))
    : fallback
}

export function normalizeThreadScrollPreference(value: unknown): ThreadScrollPreference {
  if (typeof value !== 'object' || value === null || !('version' in value) || value.version !== 1) {
    return { ...DEFAULT_THREAD_SCROLL_PREFERENCE }
  }
  const preference = value as Record<string, unknown>
  const postNo = preference.postNo === null
    ? null
    : typeof preference.postNo === 'number' && Number.isSafeInteger(preference.postNo) && preference.postNo > 0
      ? preference.postNo
      : null
  return {
    version: 1,
    postNo: postNo !== null && postNo <= 1000 ? postNo : null,
    offset: finiteRange(preference.offset, 0, -1_000_000, 1_000_000),
    scrollTop: finiteRange(preference.scrollTop, 0, 0, 10_000_000),
    atBottom: typeof preference.atBottom === 'boolean' ? preference.atBottom : DEFAULT_THREAD_SCROLL_PREFERENCE.atBottom,
    page: typeof preference.page === 'number' && Number.isSafeInteger(preference.page) && preference.page > 0 && preference.page <= 20 ? preference.page : 1,
    pageSize: preference.pageSize === 50 || preference.pageSize === 100 || preference.pageSize === 500 || preference.pageSize === 1000
      ? preference.pageSize
      : 50,
  }
}

export function parseThreadScrollPreference(serialized: string | null): ThreadScrollPreference {
  if (serialized === null) return { ...DEFAULT_THREAD_SCROLL_PREFERENCE }
  try {
    return normalizeThreadScrollPreference(JSON.parse(serialized))
  } catch {
    return { ...DEFAULT_THREAD_SCROLL_PREFERENCE }
  }
}

export function threadScrollPreferenceKey(accountId: string, threadId: string): string {
  const isDesktop = typeof window !== 'undefined'
    && (window.location.protocol === 'tauri:' || window.location.hostname === 'tauri.localhost')
  const scope = isDesktop ? 'desktop' : 'preview'
  return `swarm:thread-scroll:v1:${scope}:${encodeURIComponent(accountId || 'test')}:${encodeURIComponent(threadId)}`
}

export function readThreadScrollPreference(accountId: string, threadId: string): ThreadScrollPreference {
  return parseThreadScrollPreference(window.localStorage.getItem(threadScrollPreferenceKey(accountId, threadId)))
}

export function writeThreadScrollPreference(accountId: string, threadId: string, preference: ThreadScrollPreference): void {
  const normalized = normalizeThreadScrollPreference(preference)
  window.localStorage.setItem(threadScrollPreferenceKey(accountId, threadId), JSON.stringify(normalized))
}
