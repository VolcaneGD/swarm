// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  DEFAULT_THREAD_SCROLL_PREFERENCE,
  normalizeThreadScrollPreference,
  parseThreadScrollPreference,
  readThreadScrollPreference,
  threadScrollPreferenceKey,
  writeThreadScrollPreference,
} from './threadScrollPreferences'

describe('per-Thread scroll preference storage contract', () => {
  beforeEach(() => window.localStorage.clear())
  afterEach(() => window.localStorage.clear())

  it('uses the safe default for missing, malformed, and unsupported values', () => {
    expect(parseThreadScrollPreference(null)).toEqual(DEFAULT_THREAD_SCROLL_PREFERENCE)
    expect(parseThreadScrollPreference('{broken')).toEqual(DEFAULT_THREAD_SCROLL_PREFERENCE)
    expect(parseThreadScrollPreference('{"version":2,"postNo":42,"offset":5,"scrollTop":99,"atBottom":false}')).toEqual(DEFAULT_THREAD_SCROLL_PREFERENCE)
    expect(parseThreadScrollPreference('null')).toEqual(DEFAULT_THREAD_SCROLL_PREFERENCE)
    expect(parseThreadScrollPreference('[1,2,3]')).toEqual(DEFAULT_THREAD_SCROLL_PREFERENCE)
  })

  it('accepts a safe Post anchor and finite bounded numeric offsets only', () => {
    expect(normalizeThreadScrollPreference({ version: 1, postNo: 0, offset: Number.NaN, scrollTop: Number.POSITIVE_INFINITY, atBottom: 'yes' })).toEqual(DEFAULT_THREAD_SCROLL_PREFERENCE)
    expect(normalizeThreadScrollPreference({ version: 1, postNo: 1.5, offset: Number.NEGATIVE_INFINITY, scrollTop: -20, atBottom: false })).toEqual({
      version: 1, postNo: null, offset: 0, scrollTop: 0, atBottom: false, page: 1, pageSize: 50,
    })
    expect(normalizeThreadScrollPreference({ version: 1, postNo: Number.MAX_SAFE_INTEGER + 1, offset: 2_000_000, scrollTop: 20_000_000, atBottom: false })).toEqual({
      version: 1, postNo: null, offset: 1_000_000, scrollTop: 10_000_000, atBottom: false, page: 1, pageSize: 50,
    })
    expect(normalizeThreadScrollPreference({ version: 1, postNo: 50, offset: -12.5, scrollTop: 321.25, atBottom: false })).toEqual({
      version: 1, postNo: 50, offset: -12.5, scrollTop: 321.25, atBottom: false, page: 1, pageSize: 50,
    })
  })

  it('persists only supported page sizes and positive integer page numbers', () => {
    expect(normalizeThreadScrollPreference({ version: 1, page: 7, pageSize: 500 })).toMatchObject({ page: 7, pageSize: 500 })
    expect(normalizeThreadScrollPreference({ version: 1, page: Number.MAX_SAFE_INTEGER + 1, pageSize: 75 })).toMatchObject({ page: 1, pageSize: 50 })
    expect(normalizeThreadScrollPreference({ version: 1, page: 0, pageSize: 1000 })).toMatchObject({ page: 1, pageSize: 1000 })
    expect(normalizeThreadScrollPreference({ version: 1, page: 3.5, pageSize: '500' })).toMatchObject({ page: 1, pageSize: 50 })
    expect(normalizeThreadScrollPreference({ version: 1, postNo: 45, offset: 6, scrollTop: 120, atBottom: false })).toMatchObject({ page: 1, pageSize: 50 })
  })

  it('isolates records by immutable account id and Thread id', () => {
    const accountAThreadA = threadScrollPreferenceKey('account-a', 'thread-a')
    expect(accountAThreadA).not.toBe(threadScrollPreferenceKey('account-b', 'thread-a'))
    expect(accountAThreadA).not.toBe(threadScrollPreferenceKey('account-a', 'thread-b'))
    writeThreadScrollPreference('account-a', 'thread-a', { version: 1, postNo: 51, offset: 8, scrollTop: 300, atBottom: false })
    expect(readThreadScrollPreference('account-a', 'thread-a')).toEqual({ version: 1, postNo: 51, offset: 8, scrollTop: 300, atBottom: false, page: 1, pageSize: 50 })
    expect(readThreadScrollPreference('account-b', 'thread-a')).toEqual(DEFAULT_THREAD_SCROLL_PREFERENCE)
    expect(readThreadScrollPreference('account-a', 'thread-b')).toEqual(DEFAULT_THREAD_SCROLL_PREFERENCE)
  })

  it('drops non-schema content such as a Post body from persisted scroll records', () => {
    const preference = {
      version: 1, postNo: 51, offset: 8, scrollTop: 300, atBottom: false, page: 4, pageSize: 1000, body: 'private post body',
    } as unknown as ReturnType<typeof normalizeThreadScrollPreference>
    writeThreadScrollPreference('account-a', 'thread-a', preference)
    const stored = window.localStorage.getItem(threadScrollPreferenceKey('account-a', 'thread-a'))!
    expect(stored).not.toContain('private post body')
    expect(JSON.parse(stored)).toEqual({ version: 1, postNo: 51, offset: 8, scrollTop: 300, atBottom: false, page: 4, pageSize: 1000 })
  })

  it('falls back safely when persisted anchor or numeric fields are invalid', () => {
    const key = threadScrollPreferenceKey('account-a', 'thread-a')
    window.localStorage.setItem(key, '{"version":1,"postNo":2.5,"offset":"nearby","scrollTop":-1,"atBottom":"false"}')
    expect(readThreadScrollPreference('account-a', 'thread-a')).toEqual(DEFAULT_THREAD_SCROLL_PREFERENCE)
    window.localStorage.setItem(key, '{"version":1,"postNo":51,"offset":null,"scrollTop":321,"atBottom":false}')
    expect(readThreadScrollPreference('account-a', 'thread-a')).toEqual({ version: 1, postNo: 51, offset: 0, scrollTop: 321, atBottom: false, page: 1, pageSize: 50 })
  })
})
