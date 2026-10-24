// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest'
import {
  conversationViewPreferenceKey,
  readConversationViewPreference,
  writeConversationViewPreference,
} from './conversationViewPreferences'

describe('conversation view preferences', () => {
  beforeEach(() => window.localStorage.clear())

  it('defaults to Timeline and keeps view modes separate by account', () => {
    expect(readConversationViewPreference('account one')).toBe('timeline')
    expect(conversationViewPreferenceKey('account one')).not.toBe(conversationViewPreferenceKey('account two'))

    writeConversationViewPreference('account one', 'threaded')

    expect(readConversationViewPreference('account one')).toBe('threaded')
    expect(readConversationViewPreference('account two')).toBe('timeline')
  })

  it('restores the persisted mode after a view switch and remount-style reread', () => {
    writeConversationViewPreference('account-a', 'threaded')
    expect(readConversationViewPreference('account-a')).toBe('threaded')
    writeConversationViewPreference('account-a', 'timeline')
    expect(readConversationViewPreference('account-a')).toBe('timeline')
  })

  it('treats unknown stored values as the safe Timeline default', () => {
    window.localStorage.setItem(conversationViewPreferenceKey('account-a'), 'unknown-view')
    expect(readConversationViewPreference('account-a')).toBe('timeline')
  })
})
