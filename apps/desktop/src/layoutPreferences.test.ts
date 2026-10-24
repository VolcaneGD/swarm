// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  DEFAULT_LAYOUT_PREFERENCES,
  layoutPreferencesKey,
  normalizeLayoutPreferences,
  parseLayoutPreferences,
  readLayoutPreferences,
  writeLayoutPreferences,
} from './layoutPreferences'

describe('layout preference storage contract', () => {
  beforeEach(() => window.localStorage.clear())
  afterEach(() => window.localStorage.clear())

  it('uses safe defaults for missing, malformed, or unsupported stored schemas', () => {
    expect(parseLayoutPreferences(null)).toEqual(DEFAULT_LAYOUT_PREFERENCES)
    expect(parseLayoutPreferences('{broken')).toEqual(DEFAULT_LAYOUT_PREFERENCES)
    expect(parseLayoutPreferences('{"version":2,"treeWidth":400,"composerHeight":300}')).toEqual(DEFAULT_LAYOUT_PREFERENCES)
    expect(parseLayoutPreferences('null')).toEqual(DEFAULT_LAYOUT_PREFERENCES)
    expect(parseLayoutPreferences('[1,2,3]')).toEqual(DEFAULT_LAYOUT_PREFERENCES)
  })

  it('accepts only finite numeric sizes and clamps each value to its supported range', () => {
    expect(normalizeLayoutPreferences({ version: 1, treeWidth: '300', composerHeight: Number.NaN })).toEqual({
      version: 1, treeWidth: 280, composerHeight: 230,
    })
    expect(normalizeLayoutPreferences({ version: 1, treeWidth: Number.POSITIVE_INFINITY, composerHeight: Number.NEGATIVE_INFINITY })).toEqual({
      version: 1, treeWidth: 280, composerHeight: 230,
    })
    expect(normalizeLayoutPreferences({ version: 1, treeWidth: -1, composerHeight: 9999 })).toEqual({
      version: 1, treeWidth: 220, composerHeight: 480,
    })
    expect(normalizeLayoutPreferences({ version: 1, treeWidth: 9999, composerHeight: -1 })).toEqual({
      version: 1, treeWidth: 480, composerHeight: 200,
    })
  })

  it('falls back per field when persisted fields have invalid types or non-finite values', () => {
    const accountId = 'invalid-preferences'
    window.localStorage.setItem(layoutPreferencesKey(accountId), '{"version":1,"treeWidth":"320","composerHeight":9999}')
    expect(readLayoutPreferences(accountId)).toEqual({ version: 1, treeWidth: 280, composerHeight: 480 })

    window.localStorage.setItem(layoutPreferencesKey(accountId), '{"version":1,"treeWidth":350,"composerHeight":null}')
    expect(readLayoutPreferences(accountId)).toEqual({ version: 1, treeWidth: 350, composerHeight: 230 })
  })

  it('keeps preference records separate for different account ids and writes a normalized schema', () => {
    const accountA = 'account/a'
    const accountB = 'account-b'
    expect(layoutPreferencesKey(accountA)).not.toBe(layoutPreferencesKey(accountB))
    writeLayoutPreferences(accountA, { version: 1, treeWidth: Number.POSITIVE_INFINITY, composerHeight: 620 })
    expect(JSON.parse(window.localStorage.getItem(layoutPreferencesKey(accountA))!)).toEqual({ version: 1, treeWidth: 280, composerHeight: 480 })
    expect(readLayoutPreferences(accountB)).toEqual(DEFAULT_LAYOUT_PREFERENCES)
  })
})
