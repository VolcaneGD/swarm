export type ConversationView = 'timeline' | 'threaded'

export function conversationViewPreferenceKey(accountId: string): string {
  const desktop = window.location.protocol === 'tauri:' || window.location.hostname === 'tauri.localhost'
  return `swarm:conversation-view:v1:${desktop ? 'desktop' : 'preview'}:${encodeURIComponent(accountId || 'test')}`
}

export function normalizeConversationView(value: unknown): ConversationView {
  return value === 'threaded' ? 'threaded' : 'timeline'
}

export function parseConversationViewPreference(serialized: string | null): ConversationView {
  return normalizeConversationView(serialized)
}

export function readConversationViewPreference(accountId: string): ConversationView {
  return parseConversationViewPreference(window.localStorage.getItem(conversationViewPreferenceKey(accountId)))
}

export function writeConversationViewPreference(accountId: string, view: ConversationView): void {
  window.localStorage.setItem(conversationViewPreferenceKey(accountId), normalizeConversationView(view))
}
