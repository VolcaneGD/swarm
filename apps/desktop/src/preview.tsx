import React from 'react'
import ReactDOM from 'react-dom/client'
import { mockIPC } from '@tauri-apps/api/mocks'
import type { Account, ActivityItem, Board, ConversationCapabilities, CreationStatus, DeletedConversations, GuiSnapshot, Post, PostHistoryItem, RuntimeStatus, SearchItem, Thread } from './types'
import './styles.css'
import { normalizeSafeExternalUrl } from './safe-links'
import { DEFAULT_RULES_TEMPLATE } from './rulesDraft'
import { pickPreviewBackupFile } from './previewBackupPicker'

const banner = document.getElementById('preview-banner')
if (banner) {
  const reserveBannerSpace = () => document.documentElement.style.setProperty('--preview-banner-height', `${banner.getBoundingClientRect().height}px`)
  reserveBannerSpace()
  new ResizeObserver(reserveBannerSpace).observe(banner)
}

const now = '2026-10-08T09:00:00.000Z'
const sampleProjects: import('./bridge').Project[] = []
const longThreadSample = new URLSearchParams(window.location.search).get('sample') === 'long-thread'
const markdownSample = new URLSearchParams(window.location.search).get('sample') === 'markdown'
const threadedSample = new URLSearchParams(window.location.search).get('sample') === 'threaded'
const settingsMemberSample = new URLSearchParams(window.location.search).get('sample') === 'settings-member'
const notificationScenario = new URLSearchParams(window.location.search).get('notification')
let notificationPending = Boolean(notificationScenario)
let notificationReadFailure = notificationScenario === 'failed-read'
let sampleNotificationSettings = { enabled: true, include_excerpt: false }
let sampleDisplaySettings: import('./types').DisplaySettings | null = null
let sampleBackupSettings: import('./bridge').BackupSettings = { enabled: true, interval_seconds: 86400, retention: 7, destination: null }
let sampleCleanupArchives = [{ directory: 'C:\\Users\\Preview\\AppData\\Local\\Swarm\\backups', key: 'scheduled-00000000-0000-4000-8000-000000000001', bytes: 1048576, deletable: true }]
let sampleRecoveryBytes = 2048
let sampleManualBackup: import('./bridge').ManualBackupResult | null = null
let sampleRestore: import('./bridge').RestoreStatus | null = null
const sampleSubscriptions = new Set<string>()
const sampleReadPosts = new Set<string>()
const sampleAccount: Account = { id: settingsMemberSample ? 'sample-settings-member' : threadedSample ? 'sample-threaded-admin' : longThreadSample ? 'sample-scroll-admin' : 'sample-admin', name: settingsMemberSample ? 'Preview Member' : 'Preview Admin', is_admin: !settingsMemberSample, disabled: false, role_ids: settingsMemberSample ? [] : ['workspace-admin'] }
const sampleAccounts: Account[] = [sampleAccount,
  { id: 'sample-member', name: 'Mika', is_admin: false, disabled: false, role_ids: [] },
  { id: 'sample-disabled', name: 'Jordan', is_admin: false, disabled: true, role_ids: [] },
]
sampleAccount.lock_enabled = true
sampleAccount.google_linked = false
let samplePassword = 'preview-password'
const sampleRoles = [{ id: 'role-editor', name: 'Editors', created_at: now }]
// UI-only fixture. Rust remains the authoritative parser and permission evaluator.
const sampleRulesActions = ['board.list', 'board.get', 'thread.list', 'thread.get', 'post.list', 'post.get', 'post.create', 'post.edit', 'response.create', 'search', 'changes.get', 'rules.check', 'rules.validate', 'project.register']
const sampleRules = new Map<string, { source: string; revision: string }>([
  ['', { source: DEFAULT_RULES_TEMPLATE, revision: 'preview-global-1' }],
  ['project-preview', { source: '', revision: 'preview-project-missing' }],
])
let sampleRulesRevision = 1
let sampleRulesConflict = new URLSearchParams(window.location.search).get('sample') === 'rules-conflict'
function previewRulesDocument(projectId: unknown) {
  const key = typeof projectId === 'string' ? projectId : ''
  const item = sampleRules.get(key)
  if (!item) throw { code: 'NOT_FOUND', exit_code: 5, message: 'Sample rules scope was not found.' }
  return { ...item, scope: key ? 'project' : 'global', project_id: key || null }
}
function previewRulesValidation(source: unknown) {
  const valid = typeof source === 'string' && new TextEncoder().encode(source).length <= 1048576 && source.includes('schema_version: 1') && /rules:/.test(source) && /(?:~~~|```)swarm-rules/.test(source)
  return { valid, message: valid ? 'Preview validation succeeded. Runtime validation uses the Rust parser.' : 'Preview validation failed.', actions: valid ? sampleRulesActions.filter(action => String(source).includes(`"${action}"`) || String(source).includes(`[${action}]`)) : [] }
}
const samplePermissions = new Map<string, { operation: string; scope: string; target_id: string; decision: string }[]>()
const samplePermissionOperations = ['read', 'board.get', 'thread.get', 'post.create', 'post.edit', 'response.create', 'account.list', 'role.assign', 'permission.set'].map((operation) => ({ operation, scopes: ['read', 'board.get', 'thread.get', 'post.create', 'post.edit', 'response.create'].includes(operation) ? ['global', 'board', 'thread'] : ['global'] }))
const boards: Board[] = [
  { id: 'board-design', name: 'Product design', description: 'Ideas, decisions, and interface reviews.', created_at: now },
  { id: 'board-research', name: 'Research notes', description: 'Findings from current exploration.', created_at: now },
  { id: 'board-trash-preview', name: 'Old prototype board', description: 'Preview-only deleted Board for reviewing the trash actions.', created_at: now },
]
const threads: Thread[] = [
  { id: 'thread-onboarding', board_id: 'board-design', name: 'A calmer onboarding flow', description: 'Discussing the first-run experience.', created_at: now },
  { id: 'thread-navigation', board_id: 'board-design', name: 'Navigation labels', description: 'Names for the workspace sections.', created_at: now },
  { id: 'thread-retrieval', board_id: 'board-research', name: 'Retrieval quality', description: 'Notes on useful context and citations.', created_at: now },
  { id: 'thread-archive', board_id: 'board-design', name: 'Past design discussion', description: 'Sample Thread with 1,000 allocated post numbers.', created_at: now, archived: true },
  { id: 'thread-trash-preview', board_id: 'board-trash-preview', name: 'Deleted prototype review', description: 'Preview-only deleted child Thread.', created_at: now },
  { id: 'thread-trash-sibling', board_id: 'board-trash-preview', name: 'Hidden sibling review', description: 'Preview-only non-deleted child for Board purge cascade.', created_at: now },
]
const user = { id: sampleAccount.id, model_name: 'USER', display_id: 'abcde12345', user_name: sampleAccount.name }
const agent = { id: 'sample-agent', model_name: 'Example Model', display_id: 'f012345678', harness_name: 'Antigravity' }
const posts: Post[] = [
  { id: 'post-onboarding-1', thread_id: 'thread-onboarding', no: 1, body: 'Could we make the first screen explain what stays on this device?', author: user, created_at: now, updated_at: now, revision: 2, reply_to: null, edited_by: 'sample-admin', editor_display_id: 'Preview Admin', editor_model_name: 'User', deleted: false, deleted_at: null, response_count: 2 },
  { id: 'post-onboarding-2', thread_id: 'thread-onboarding', no: 2, body: 'A short privacy note beside the local setup step should help.', author: agent, created_at: now, updated_at: now, revision: 1, reply_to: 1, edited_by: null, editor_display_id: null, editor_model_name: null, deleted: false, deleted_at: null, response_count: 0 },
  { id: 'post-onboarding-3', thread_id: 'thread-onboarding', no: 3, body: null, author: user, created_at: now, updated_at: now, revision: 1, reply_to: null, edited_by: null, editor_display_id: null, editor_model_name: null, deleted: true, deleted_at: now, response_count: 1 },
  { id: 'post-onboarding-4', thread_id: 'thread-onboarding', no: 4, body: 'The banner should remain visible in the preview.', author: user, created_at: now, updated_at: now, revision: 1, reply_to: 1, edited_by: null, editor_display_id: null, editor_model_name: null, deleted: false, deleted_at: null, response_count: 0 },
  { id: 'post-navigation-1', thread_id: 'thread-navigation', no: 1, body: '“Boards” and “Threads” feel familiar. Keep the labels short.', author: user, created_at: now, updated_at: now, revision: 1, reply_to: null, edited_by: null, editor_display_id: null, editor_model_name: null, deleted: false, deleted_at: null, response_count: 0 },
  { id: 'post-retrieval-1', thread_id: 'thread-retrieval', no: 1, body: 'A useful answer should show which source supports each claim.', author: agent, created_at: now, updated_at: now, revision: 1, reply_to: null, edited_by: null, editor_display_id: null, editor_model_name: null, deleted: false, deleted_at: null, response_count: 1 },
  { id: 'post-retrieval-2', thread_id: 'thread-retrieval', no: 2, body: 'Agreed. Let’s keep retrieval and answer validation distinct. 検索と回答の検証を分けます。', author: user, created_at: now, updated_at: now, revision: 1, reply_to: 1, edited_by: null, editor_display_id: null, editor_model_name: null, deleted: false, deleted_at: null, response_count: 0 },
]
// A bounded long-thread fixture exercises pagination without changing the default preview.
if (longThreadSample || threadedSample) {
  for (let no = 5; no <= 1000; no += 1) {
    posts.push({ id: `post-onboarding-${no}`, thread_id: 'thread-onboarding', no,
      body: threadedSample ? `## Reply ${no}\n\n深い返信でも本文の幅を確保します。番号とページ送りを維持します。\n\n${'longword'.repeat(15)}` : `Sample Post ${no}: a longer conversation for scroll restoration and numbered pagination.`,
      author: no % 2 ? user : agent, created_at: now, updated_at: now, revision: 1, reply_to: threadedSample ? no === 12 ? null : no === 8 ? 1 : no - 1 : null,
      edited_by: null, editor_display_id: null, editor_model_name: null, deleted: false, deleted_at: null, response_count: 0 })
  }
}
if (threadedSample) {
  for (const post of posts) post.response_count = posts.filter(reply => reply.thread_id === post.thread_id && reply.reply_to === post.no).length
}
const histories = new Map<string, PostHistoryItem[]>([
  ['thread-onboarding:1', [{ revision: 1, body: 'Could we explain what stays on this device?', saved_at: now, editor_id: 'sample-admin', editor_display_id: 'Preview Admin', editor_model_name: 'User' }]],
])
for (let no = 1; no <= 1000; no++) {
  const id = `post-archive-${no}`
  posts.push({ id, thread_id: 'thread-archive', no, body: no === 1000 ? 'This discussion reached 1,000 posts.\n\n```typescript\nconsole.log("archived code remains readable");\n```\n\n```python\nprint("independent disclosure")\n```' : `Archived sample Post ${no}.`, author: user, created_at: now, updated_at: now, revision: 1, reply_to: null, edited_by: null, editor_display_id: null, editor_model_name: null, deleted: false, deleted_at: null, response_count: 0 })
  if (no < 1000) sampleReadPosts.add(id)
}
if (markdownSample) {
  posts[0].body = [
    '# Markdownの表示確認',
    '見出し、**強調**、*斜体*、~~取り消し線~~、`inline code`を表示します。',
    '- 表示だけを装飾\n- 保存する本文はMarkdownのまま',
    '> 引用文は本文から区別して表示します。',
    '| 項目 | 状態 |\n| --- | --- |\n| 日本語とEnglish | 対応 |\n| 外部画像 | 自動取得なし |',
    '```typescript\nconst message = "長いコードは本文の幅を広げず、この枠内で横スクロールします。";\nconsole.log(message);\n```',
    '[表示確認用リンク](https://example.org/markdown?source=swarm#preview)',
    '![画像の代替テキスト](https://example.org/never-fetch.png)',
    '<script>alert("inert")</script>',
    '[実行されないリンク](javascript:alert%281%29)',
    `長い文字列: ${'abcdefghij'.repeat(35)}`,
  ].join('\n\n')
  posts[1].body = '## ResponseのMarkdown\n\n1. 親投稿を確認\n2. **返信を表示**\n\n> 返信でもHTMLは実行しません。'
  histories.get('thread-onboarding:1')![0].body = '## 編集前の本文\n\n- **履歴**でも装飾を表示\n\n```text\noriginal Markdown source\n```'
}
const capabilities: ConversationCapabilities = {
  board_create: true,
  boards: boards.map((board) => ({ id: board.id, thread_create: true, board_delete: true })),
  threads: threads.map((thread) => ({ id: thread.id, post_create: true, response_create: true, post_edit: true, post_history: true, thread_delete: true, post_delete: true, post_restore: true })),
}
const deletedAt = new Map<string, string>([['board:board-trash-preview', now], ['thread:thread-trash-preview', now]])
const deletedBodies = new Map<string, string>()
let entityNumber = 0

const setupSample = new URLSearchParams(window.location.search).get('sample') === 'setup'
let snapshot: GuiSnapshot = { setupRequired: setupSample, account: setupSample ? null : sampleAccount, pendingIssue: new URLSearchParams(window.location.search).get('sample') === 'pending-auth', accountNameHistory: setupSample ? [] : [sampleAccount.name, 'Preview Member'] }
let sampleRuntimeState: RuntimeStatus['state'] = new URLSearchParams(window.location.search).get('sample') === 'runtime-stopped' ? 'stopped' : 'running'
let sampleStartupEnabled = true
let sampleStartupRegistered = new URLSearchParams(window.location.search).get('sample') !== 'startup-mismatch'
const creationStatus: CreationStatus = { pending: false, operation: null, expired: false }
const visibleBoards = () => boards.filter((board) => !deletedAt.has(`board:${board.id}`))
const visibleThreads = () => threads.filter((thread) => !deletedAt.has(`thread:${thread.id}`) && (!thread.board_id || visibleBoards().some((board) => board.id === thread.board_id)))
const visiblePosts = (threadId: string) => posts.filter((post) => post.thread_id === threadId)
const postByNo = (threadId: string, no: number) => posts.find((post) => post.thread_id === threadId && post.no === no)
const pinStorageKey = `swarm:preview-pins:v1:${sampleAccount.id}`
const loadPins = (): Set<string> => {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(pinStorageKey) || '[]')
    return new Set(Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string') : [])
  } catch { return new Set() }
}
let pinnedIds = loadPins()
const activityItems: ActivityItem[] = []
const recordActivity = (action: string, target: Board | Thread | Post, occurredAt = new Date().toISOString()) => {
  const post = 'thread_id' in target ? target : null
  const thread = post ? threads.find((item) => item.id === post.thread_id) : 'board_id' in target ? target : null
  const board = thread ? boards.find((item) => item.id === thread.board_id) : 'name' in target && !('board_id' in target) ? target : null
  activityItems.push({ id: activityItems.length + 1, action, occurred_at: occurredAt,
    thread_id: thread?.id ?? null, thread_name: thread?.name ?? null,
    board_id: board?.id ?? null, board_name: board?.name ?? null, post_no: post?.no ?? null })
}
boards.forEach((board) => recordActivity('board.create', board, now))
threads.forEach((thread) => recordActivity('thread.create', thread, now))
posts.forEach((post) => recordActivity(post.reply_to === null ? 'post.create' : 'response.create', post, now))
recordActivity('post.edit', posts[0], now)
recordActivity('post.delete', posts[2], now)
const activeDeletedConversations = (): DeletedConversations => ({
  boards: boards.filter((board) => deletedAt.has(`board:${board.id}`)).map((board) => ({ ...board, deleted_at: deletedAt.get(`board:${board.id}`)!, can_restore: true, can_purge: true })),
  threads: threads.filter((thread) => deletedAt.has(`thread:${thread.id}`)).map((thread) => ({ ...thread, deleted_at: deletedAt.get(`thread:${thread.id}`)!, parent_deleted: thread.board_id ? deletedAt.has(`board:${thread.board_id}`) : false, can_restore: true, can_purge: true })),
})
function purgeSampleConversation(kind: 'board' | 'thread', id: string) {
  const threadIds = kind === 'board' ? threads.filter((thread) => thread.board_id === id).map((thread) => thread.id) : [id]
  const threadIdSet = new Set(threadIds)
  const postIds = new Set(posts.filter((post) => threadIdSet.has(post.thread_id)).map((post) => post.id))
  const postKeys = posts.filter((post) => threadIdSet.has(post.thread_id)).map((post) => `${post.thread_id}:${post.no}`)
  if (kind === 'board') {
    const index = boards.findIndex((board) => board.id === id)
    if (index >= 0) boards.splice(index, 1)
    deletedAt.delete(`board:${id}`)
  }
  for (const threadId of threadIds) {
    const index = threads.findIndex((thread) => thread.id === threadId)
    if (index >= 0) threads.splice(index, 1)
    deletedAt.delete(`thread:${threadId}`)
    sampleSubscriptions.delete(threadId)
    sampleReadPosts.forEach((postId) => { if (postIds.has(postId)) sampleReadPosts.delete(postId) })
  }
  for (const postId of postIds) deletedBodies.delete(postId)
  for (const key of postKeys) histories.delete(key)
  for (let index = posts.length - 1; index >= 0; index--) if (threadIdSet.has(posts[index].thread_id)) posts.splice(index, 1)
  for (const threadId of threadIds) pinnedIds.delete(threadId)
  localStorage.setItem(pinStorageKey, JSON.stringify([...pinnedIds]))
  for (let index = activityItems.length - 1; index >= 0; index--) {
    const item = activityItems[index]
    if (item.thread_id && threadIdSet.has(item.thread_id) || kind === 'board' && item.board_id === id) activityItems.splice(index, 1)
  }
}
const getCapabilities = (): ConversationCapabilities => ({
  ...capabilities,
  boards: visibleBoards().map((board) => capabilities.boards.find((item) => item.id === board.id) || { id: board.id, thread_create: true, board_delete: true }),
  threads: visibleThreads().map((thread) => ({ ...(capabilities.threads.find((item) => item.id === thread.id) || { id: thread.id, post_edit: true, post_history: true, thread_delete: true, post_delete: true, post_restore: true }), post_create: !thread.archived, response_create: !thread.archived })),
})
const makePost = (threadId: string, body: string, replyTo: number | null): Post => {
  const nextNo = Math.max(0, ...visiblePosts(threadId).map((post) => post.no)) + 1
  if (nextNo > 1000) throw { code: 'THREAD_FULL', exit_code: 6, message: 'This Thread has reached the limit of 1000 Posts' }
  const post: Post = { id: `sample-post-${++entityNumber}`, thread_id: threadId, no: nextNo, body, author: user, created_at: new Date().toISOString(), updated_at: new Date().toISOString(), revision: 1, reply_to: replyTo, edited_by: null, editor_display_id: null, editor_model_name: null, deleted: false, deleted_at: null, response_count: 0 }
  if (replyTo !== null) {
    const parent = postByNo(threadId, replyTo)
    if (parent) parent.response_count += 1
  }
  posts.push(post)
  if (nextNo === 1000) {
    const thread = threads.find(thread => thread.id === threadId)
    if (thread) thread.archived = true
  }
  recordActivity(replyTo === null ? 'post.create' : 'response.create', post)
  return post
}

// Read-only search fixture. Opaque cursors live only in this preview document.
const searchCursors = new Map<string, { signature: string; offset: number }>()
let searchCursorNumber = 0
function sampleSearch(args: Record<string, unknown>) {
  const query = typeof args.query === 'string' ? args.query.trim() : ''
  const scope = args.scope || 'all'
  const limit = typeof args.limit === 'number' ? args.limit : 50
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean)
  if (!terms.length || !Number.isInteger(limit) || limit < 1 || limit > 100) throw { code: 'INVALID_INPUT', exit_code: 2, message: 'Invalid sample search.' }
  const signature = JSON.stringify([query, scope, args.board_id || '', args.thread_id || '', args.project_id || '', args.author_id || '', args.from || '', args.to || '', limit])
  const cursor = typeof args.cursor === 'string' ? searchCursors.get(args.cursor) : undefined
  if (args.cursor && (!cursor || cursor.signature !== signature)) throw { code: 'INVALID_INPUT', exit_code: 2, message: 'Sample search cursor is no longer valid.' }
  const candidates: SearchItem[] = []
  const dateMatches = (date: string) => (!args.from || Date.parse(date) >= Date.parse(String(args.from))) && (!args.to || Date.parse(date) <= Date.parse(String(args.to)))
  const add = (item: SearchItem, text: string) => {
    const matchesKind = scope === 'all' || scope === item.kind || (scope === 'body' && (item.kind === 'post' || item.kind === 'response')) || (scope === 'board_name' && item.kind === 'board') || (scope === 'thread_name' && item.kind === 'thread')
    const searchable = scope === 'board_name' || scope === 'thread_name' ? item.name || '' : text
    if (matchesKind && dateMatches(item.created_at) && terms.every((term) => searchable.toLowerCase().includes(term))) candidates.push(item)
  }
  const excerpt = (text: string) => Array.from(text).slice(0, 240).join('')
  if (!args.author_id && !args.project_id && !args.thread_id) {
    for (const board of visibleBoards()) {
      if (args.board_id && args.board_id !== board.id) continue
      add({ kind: 'board', id: board.id, board_id: board.id, thread_id: null, name: board.name, excerpt: excerpt(board.description), post_no: null, created_at: board.created_at }, `${board.name}\n${board.description}`)
    }
  }
  for (const thread of visibleThreads()) {
    if ((args.board_id && args.board_id !== thread.board_id) || (args.thread_id && args.thread_id !== thread.id)) continue
    if (!args.author_id && !args.project_id) add({ kind: 'thread', id: thread.id, board_id: thread.board_id, thread_id: thread.id, name: thread.name, excerpt: excerpt(thread.description), post_no: null, created_at: thread.created_at }, `${thread.name}\n${thread.description}`)
    for (const post of visiblePosts(thread.id)) {
      if (post.deleted || !post.body || (args.author_id && args.author_id !== post.author.id)) continue
      if (args.project_id && !(args.project_id === 'sample-project' && post.author.id === agent.id)) continue
      add({ kind: post.reply_to === null ? 'post' : 'response', id: post.id, board_id: thread.board_id, thread_id: thread.id, name: thread.name, excerpt: excerpt(post.body), post_no: post.no, created_at: post.created_at }, post.body)
    }
  }
  candidates.sort((a, b) => b.created_at.localeCompare(a.created_at) || b.id.localeCompare(a.id) || b.kind.localeCompare(a.kind))
  const offset = cursor?.offset || 0
  const items = candidates.slice(offset, offset + limit)
  let next_cursor: string | null = null
  if (offset + limit < candidates.length) {
    next_cursor = `sample-search-${++searchCursorNumber}`
    searchCursors.set(next_cursor, { signature, offset: offset + limit })
  }
  return { items, next_cursor, total_count: candidates.length }
}

let sampleRetention: import('./bridge').RetentionSettings = { history_days: null, deleted_days: null, deleted_board_days: null, deleted_thread_days: null, archived_thread_days: null }
mockIPC((command, payload) => {
  const request = !Array.isArray(payload) && typeof payload === 'object' ? payload as Record<string, unknown> : {}
  const args = (request.args || {}) as Record<string, unknown>
  switch (command) {
    case 'gui_retention_control': {
      if (!snapshot.account?.is_admin || request.expectedAccountId !== snapshot.account.id) throw { code: 'PERMISSION_DENIED', exit_code: 4, message: 'Retention permission required.' }
      if (request.operation === 'retention.settings.set') sampleRetention = args as import('./bridge').RetentionSettings
      return { ...sampleRetention }
    }
    case 'gui_restore_status': return sampleRestore
    case 'gui_restore_apply': {
      if (!snapshot.account?.is_admin || request.expectedAccountId !== snapshot.account.id || sampleRestore?.state !== 'prepared' || request.confirmed !== true) throw new Error('Invalid sample restore application')
      sampleRestore = { ...sampleRestore, state: 'applied', can_retry: false, preparation: sampleRestore.preparation ? { ...sampleRestore.preparation, state: 'applied' } : null }
      snapshot = { ...snapshot, account: null }
      return { state: 'applied', reauthentication_required: true }
    }
    case 'gui_restore_clear': sampleRestore = null; return null
    case 'gui_restore_pick': return (async () => {
      if (!snapshot.account?.is_admin || request.expectedAccountId !== snapshot.account.id) throw new Error('Sample administrator required')
      const name = await pickPreviewBackupFile()
      if (name === null) return null
      // Native selection rechecks account too; preserve existing fixture on cancel/change.
      if (!snapshot.account?.is_admin || request.expectedAccountId !== snapshot.account.id) throw new Error('Sample administrator changed')
      sampleRestore = { state: 'selected', path: `Preview: ${name}`, error_code: null, preparation: null,
        preview: { input_sha256: 'a'.repeat(64), targets: [{ project_id: null, target_path: 'C:\\Users\\Preview\\AppData\\Local\\Swarm\\RULES.md', revision: 'sample-global' }, { project_id: 'sample-project', target_path: 'C:\\Projects\\Swarm-preview-with-a-long-project-directory-name\\RULES.md', revision: 'missing:v1' }], manifest: { created_at: now, schema_version: 7, database_bytes: 1024000, rules: [{ project_id: null, source_path: 'C:\\OldSwarm\\RULES.md', revision: 'sample-global', content: DEFAULT_RULES_TEMPLATE }, { project_id: 'sample-project', source_path: 'C:\\Projects\\Swarm-preview-with-a-long-project-directory-name\\RULES.md', revision: 'missing:v1', content: null }] } } }
      return sampleRestore
    })()
    case 'gui_restore_prepare': {
      if (!snapshot.account?.is_admin || request.expectedAccountId !== snapshot.account.id || !sampleRestore || request.confirmed !== true) throw new Error('Invalid sample restore preparation')
      const intent = sampleRestore.preparation?.intent ?? crypto.randomUUID()
      sampleRestore = { ...sampleRestore, state: 'prepared', can_retry: true, preparation: { intent, state: 'prepared', safety_directory: 'C:\\Users\\Preview\\AppData\\Local\\Swarm\\restore-safety', safety_key: `safety-${intent}`, safety_sha256: 'b'.repeat(64) } }
      return sampleRestore
    }
    case 'gui_backup_control': {
      if (!snapshot.account?.is_admin || request.expectedAccountId !== snapshot.account.id) throw { code: 'PERMISSION_DENIED', exit_code: 4, message: 'Administrator required.' }
      if (request.operation === 'archives.list') return { archives: sampleCleanupArchives.map((archive) => ({ ...archive })), recovery_bytes: sampleRecoveryBytes }
      if (request.operation === 'archives.delete') {
        if (args.confirmed !== true || typeof args.key !== 'string') throw new Error('Invalid sample archive deletion')
        if (!sampleCleanupArchives.some((archive) => archive.key === args.key)) throw new Error('Stale sample archive key')
        sampleCleanupArchives = sampleCleanupArchives.filter((archive) => archive.key !== args.key)
        return { deleted: true }
      }
      if (request.operation === 'restore.cleanup') {
        if (args.confirmed !== true) throw new Error('Invalid sample restore cleanup')
        sampleRecoveryBytes = 0
        return { deleted: true }
      }
      if (request.operation === 'settings.set') { sampleBackupSettings = args as import('./bridge').BackupSettings; return { ...sampleBackupSettings } }
      if (request.operation === 'manual.create') {
        if (!sampleManualBackup) {const intent=crypto.randomUUID();sampleManualBackup={intent,state:'completed',key:`manual-${intent}`,destination:sampleBackupSettings.destination ?? 'C:\\Users\\Preview\\AppData\\Local\\Swarm\\backups'}}
        return sampleManualBackup
      }
      if (request.operation === 'manual.ack') {if (args.intent !== sampleManualBackup?.intent) throw new Error('Stale manual intent');sampleManualBackup=null;return {acknowledged:true}}
      return { manual:sampleManualBackup,settings: sampleBackupSettings, default_destination: 'C:\\Users\\Preview\\AppData\\Local\\Swarm\\backups', journal_error: null, pending_stage: null, schedule: { version: 1, last_success_at: 1791522000, pending: null, generations: [{ key: 'scheduled-00000000-0000-4000-8000-000000000001', destination: 'C:\\Users\\Preview\\AppData\\Local\\Swarm\\backups' }], prune: [] } }
    }
    case 'gui_display_defaults': return { language: 'en', theme: 'light' }
    case 'gui_notification_activation':
      if (!notificationPending) return { state: 'none' }
      if (notificationScenario === 'account') return { state: 'account_required', event_id: 'preview-notification' }
      if (notificationScenario === 'unavailable') return { state: 'unavailable', event_id: 'preview-notification' }
      return { state: 'ready', event_id: 'preview-notification', account_id: sampleAccount.id, thread: threads[2] }
    case 'gui_notification_ack': notificationPending = false; return true
    case 'gui_open_link':
      if (!normalizeSafeExternalUrl(String(request.url ?? ''))) throw new Error('Invalid external link')
      return null // Preview verifies the UI only; it never starts an external browser.
    case 'gui_check_update': return new URLSearchParams(window.location.search).get('update') === 'available' ? { version: '0.2.0' } : null
    case 'gui_update_notes': {
      if (new URLSearchParams(window.location.search).get('update') !== 'completed' || sessionStorage.getItem('swarm-preview-update-notes-shown')) return null
      // Consume once at boot; React retains the result until this window reloads.
      sessionStorage.setItem('swarm-preview-update-notes-shown', 'true')
      return { version: '0.2.0', notes: 'プレビュー用のアップデート内容です。\n・起動時のアップデート案内を追加\n・ダウンロードの進捗表示を追加\n・更新後のアップデート内容表示を追加' }
    }
    case 'gui_install_update': return (async () => {
      const channel = request.onProgress as { onmessage: (progress: import('./updates').UpdateProgress) => void }
      // Preview-only simulation, not network, installation or native acceptance.
      for (let downloaded = 0; downloaded <= 100; downloaded += 10) {
        channel.onmessage({ phase: 'downloading', downloaded, total: 100 })
        await new Promise(resolve => setTimeout(resolve, 200))
      }
      channel.onmessage({ phase: 'verifying' })
      await new Promise(resolve => setTimeout(resolve, 400))
      channel.onmessage({ phase: 'installing' })
      await new Promise(resolve => setTimeout(resolve, 400))
    })()
    case 'gui_restart_after_update': return new Promise<void>(resolve => {
      setTimeout(() => {
        const url = new URL(window.location.href)
        url.searchParams.set('update', 'completed')
        sessionStorage.removeItem('swarm-preview-update-notes-shown')
        window.location.replace(url)
        resolve()
      }, 700)
    })
    case 'gui_snapshot': return snapshot
    case 'gui_google_link':
    case 'gui_google_unlink': {
      if(!snapshot.account || request.expectedAccountId!==snapshot.account.id || (snapshot.account.lock_enabled && request.currentPassword!==samplePassword)) throw {code:'AUTHENTICATION_REQUIRED',exit_code:3,message:'Sample current password is invalid.'}
      sampleAccount.google_linked=command==='gui_google_link'
      sampleAccount.google_email=sampleAccount.google_linked?'preview@example.test':null
      snapshot.account={...snapshot.account,google_linked:sampleAccount.google_linked,google_email:sampleAccount.google_email}
      return {...snapshot}
    }
    case 'gui_google_recover': {
      if(!sampleAccount.google_linked || request.name!==sampleAccount.name || typeof request.password!=='string' || !request.password.trim()) throw {code:'AUTHENTICATION_REQUIRED',exit_code:3,message:'Sample recovery is unavailable.'}
      samplePassword=request.password; sampleAccount.lock_enabled=true
      snapshot.account={...sampleAccount}; snapshot.setupRequired=false; snapshot.pendingIssue=false
      return {...snapshot}
    }
    case 'gui_set_password': {
      if (!snapshot.account || request.expectedAccountId !== snapshot.account.id || typeof request.password !== 'string' || typeof request.currentPassword !== 'string') throw new Error('Invalid sample account.')
      if (snapshot.account.lock_enabled && request.currentPassword !== samplePassword) throw new Error('Sample current password is incorrect.')
      samplePassword = request.password
      snapshot.account = { ...snapshot.account, lock_enabled: request.password !== '' }
      return { ...snapshot }
    }
    case 'gui_runtime_status': return { state: sampleRuntimeState }
    case 'gui_runtime_start':
      sampleRuntimeState = 'running'
      return { state: sampleRuntimeState }
    case 'gui_runtime_stop':
      sampleRuntimeState = 'stopped'
      return { state: sampleRuntimeState }
    case 'gui_startup_status':
      return { enabled: sampleStartupEnabled, registered: sampleStartupRegistered, configured: true }
    case 'gui_startup_set':
      sampleStartupEnabled = Boolean(request.enabled)
      sampleStartupRegistered = sampleStartupEnabled
      return { enabled: sampleStartupEnabled, registered: sampleStartupRegistered, configured: true }
    case 'gui_creation_status': return creationStatus
    case 'gui_read': {
      if (request.operation === 'post.page' && notificationReadFailure) {
        notificationReadFailure = false
        throw { code: 'CONNECTION_ERROR', exit_code: 8, message: 'Preview transient read failure' }
      }
      const operation = request.operation as string
      if (operation === 'rules.capabilities') return settingsMemberSample ? { global: { read: false, validate: false, save: false, check: false }, projects: [], actions: [] } : { global: { read: true, validate: true, save: true, check: true }, projects: [{ id: 'project-preview', name: 'Preview project', read: true, validate: true, save: true, check: true }], actions: sampleRulesActions }
      if (operation === 'rules.subjects') return { items: args.project_id === 'project-preview' ? [{ id: 'ai-preview', project_id: 'project-preview', model_name: 'Preview AI' }] : [] }
      if (operation === 'rules.get') return previewRulesDocument(args.project_id)
      if (operation === 'search') return sampleSearch(args)
      if (operation === 'conversation.capabilities') return getCapabilities()
      if (operation === 'management.capabilities') return { account_list: !settingsMemberSample, account_get: !settingsMemberSample, account_create: !settingsMemberSample, account_disable: !settingsMemberSample, account_reset: !settingsMemberSample }
      if (operation === 'authorization.capabilities') return { role_create: !settingsMemberSample, role_list: !settingsMemberSample, role_assign: !settingsMemberSample, permission_set: !settingsMemberSample, permission_list: !settingsMemberSample, operations: settingsMemberSample ? [] : samplePermissionOperations }
      if (operation === 'role.list') return { items: sampleRoles.map((role) => ({ ...role })) }
      if (operation === 'permission.list') return { subject_type: args.subject_type, subject_id: args.subject_id, items: (samplePermissions.get(`${args.subject_type}:${args.subject_id}`) || []).map((permission) => ({ ...permission })) }
      if (operation === 'account.list') return { items: sampleAccounts.map((account) => ({ ...account, role_ids: [...account.role_ids] })) }
      if (operation === 'account.get') {
        const account = sampleAccounts.find((item) => item.id === args.id)
        if (!account) throw { code: 'NOT_FOUND', exit_code: 5, message: 'Sample account was not found.' }
        return { ...account, role_ids: [...account.role_ids] }
      }
      if (operation === 'conversation.deleted') return activeDeletedConversations()
      if (operation === 'conversation.pins') return { items: visibleThreads().filter((thread) => pinnedIds.has(thread.id)) }
      if (operation === 'notification.settings.get') return { ...sampleNotificationSettings }
      if (operation === 'display.settings.get') return { settings: sampleDisplaySettings }
      if (operation === 'conversation.subscriptions') return { items: visibleThreads().filter((thread) => sampleSubscriptions.has(thread.id)).map(({ id }) => ({ id })) }
      if (operation === 'conversation.unread') {
        const items = visibleThreads().map((thread) => ({ thread_id: thread.id, unread_count: visiblePosts(thread.id).filter((post) => !post.deleted && post.body !== null && !sampleReadPosts.has(post.id)).length }))
        return { items, total: items.reduce((count, item) => count + item.unread_count, 0) }
      }
      if (operation === 'conversation.activity') {
        const before = typeof args.before_id === 'number' ? args.before_id : Number.POSITIVE_INFINITY
        const limit = typeof args.limit === 'number' ? args.limit : 50
        const candidates = activityItems.filter((item) => item.id < before).reverse()
        const items = candidates.slice(0, limit)
        return { items, next_before_id: candidates.length > limit ? items.at(-1)?.id ?? null : null }
      }
      if (operation === 'board.list') return { items: visibleBoards() }
      if (operation === 'thread.list') return { items: visibleThreads() }
      if (operation === 'thread.get') return threads.find((thread) => thread.id === args.id) || null
      if (operation === 'post.get') return postByNo(String(args.thread_id), Number(args.no)) || null
      if (operation === 'post.history') return { items: histories.get(`${args.thread_id}:${args.no}`) || [] }
      if (operation === 'post.page') {
        const items = visiblePosts(String(args.thread_id)).sort((left, right) => left.no - right.no)
        const pageSize = typeof args.page_size === 'number' ? args.page_size : 50
        const totalPages = Math.max(1, Math.ceil(items.length / pageSize))
        const anchorIndex = typeof args.anchor_no === 'number' ? items.findIndex((post) => post.no === args.anchor_no) : -1
        if (typeof args.anchor_no === 'number' && anchorIndex < 0) throw { code: 'NOT_FOUND', exit_code: 5, message: 'Post was not found.' }
        const requestedPage = anchorIndex >= 0 ? Math.floor(anchorIndex / pageSize) + 1 : typeof args.page === 'number' ? args.page : 1
        const page = Math.max(1, Math.min(totalPages, requestedPage))
        return { items: items.slice((page - 1) * pageSize, page * pageSize), page, page_size: pageSize, total_posts: items.length, total_pages: totalPages }
      }
      if (operation === 'post.list' || operation === 'response.list') {
        const threadId = String(args.thread_id)
        const before = typeof args.before_no === 'number' ? args.before_no : Number.POSITIVE_INFINITY
        let items = visiblePosts(threadId).filter((post) => post.no < before)
        if (operation === 'response.list') items = items.filter((post) => post.reply_to === Number(args.to))
        items.sort((left, right) => left.no - right.no)
        const limit = typeof args.limit === 'number' ? args.limit : 50
        const page = items.slice(-limit)
        return { items: page, next_before_no: items.length > limit ? page[0]?.no ?? null : null }
      }
      throw new Error(`Preview does not support read operation: ${operation}`)
    }
    case 'gui_project_control': {
      if (settingsMemberSample || request.expectedAccountId !== sampleAccount.id) throw { code: 'PERMISSION_DENIED', exit_code: 4, message: 'Admin required.' }
      if (request.operation === 'project.list') return { items: sampleProjects.map((item) => ({ ...item })) }
      if (request.operation !== 'project.register' || typeof args.name !== 'string' || typeof args.root !== 'string' || !args.name.trim() || !args.root.trim()) throw { code: 'INVALID_INPUT', exit_code: 2, message: 'Invalid project input.' }
      const prior = sampleProjects.find((project) => project.canonical_root === args.root)
      if (prior) return { ...prior }
      const project = { id: crypto.randomUUID(), name: args.name, canonical_root: args.root, paths: [args.root], created_at: now }
      sampleProjects.push(project)
      return { ...project }
    }
    case 'gui_account_manage': {
      if (request.operation === 'account.create') {
        const name = typeof args.name === 'string' ? args.name.trim() : ''
        if (!name || typeof args.password !== 'string' || (args.password !== '' && !args.password.trim())) throw { code: 'INVALID_INPUT', exit_code: 2, message: 'Account input is invalid.' }
        if (sampleAccounts.some((account) => account.name === name)) throw { code: 'ACCOUNT_NAME_CONFLICT', exit_code: 6, message: 'An account with this name already exists.' }
        const account: Account = { id: `sample-account-${++entityNumber}`, name, is_admin: false, disabled: false, role_ids: [], lock_enabled: args.password !== '' }
        sampleAccounts.push(account)
        return { ...account }
      }
      const account = sampleAccounts.find((item) => item.id === args.id)
      if (!account) throw { code: 'NOT_FOUND', exit_code: 5, message: 'Sample account was not found.' }
      if (account.is_admin) throw { code: 'PERMISSION_DENIED', exit_code: 4, message: 'The main Admin is protected.' }
      if (request.operation === 'account.disable' && typeof args.disabled === 'boolean') account.disabled = args.disabled
      else if (request.operation !== 'account.reset' || typeof args.password !== 'string' || !args.password.trim()) throw { code: 'INVALID_INPUT', exit_code: 2, message: 'Account input is invalid.' }
      // This preview never stores or authenticates a real password.
      return { ...account, role_ids: [...account.role_ids] }
    }
    case 'gui_notification_manage':
    case 'gui_manage': {
      // The production host validates identity and permissions; this is sample-only.
      if (command === 'gui_notification_manage' && request.expectedAccountId !== sampleAccount.id) throw { code: 'ACCOUNT_CHANGED', exit_code: 6, message: 'The selected account changed.' }
      const operation = request.operation as string
      const manageArgs = (request.args || {}) as Record<string, unknown>
      if (operation === 'conversation.batch') {
        const action = manageArgs.action
        const items = manageArgs.items
        if (manageArgs.confirmed !== true || (action !== 'restore' && action !== 'purge') || !Array.isArray(items) || items.length === 0) throw { code: 'INVALID_INPUT', exit_code: 2, message: 'Invalid sample conversation batch.' }
        const normalized = items.map((item) => {
          if (!item || typeof item !== 'object') throw { code: 'INVALID_INPUT', exit_code: 2, message: 'Invalid sample conversation target.' }
          const target = item as { kind?: unknown; id?: unknown }
          if ((target.kind !== 'board' && target.kind !== 'thread') || typeof target.id !== 'string' || !target.id) throw { code: 'INVALID_INPUT', exit_code: 2, message: 'Invalid sample conversation target.' }
          return { kind: target.kind, id: target.id }
        })
        const keys = normalized.map((item) => `${item.kind}:${item.id}`)
        if (new Set(keys).size !== keys.length) throw { code: 'INVALID_INPUT', exit_code: 2, message: 'Duplicate sample conversation target.' }
        const selectedBoards = new Set(normalized.filter((item) => item.kind === 'board').map((item) => item.id))
        for (const item of normalized) {
          const key = `${item.kind}:${item.id}`
          const exists = item.kind === 'board' ? boards.some((board) => board.id === item.id) : threads.some((thread) => thread.id === item.id)
          if (!exists || !deletedAt.has(key)) throw { code: 'STALE_TARGET', exit_code: 5, message: 'A sample conversation target is stale.' }
          if (action === 'restore' && item.kind === 'thread') {
            const thread = threads.find((candidate) => candidate.id === item.id)!
            if (thread.board_id && deletedAt.has(`board:${thread.board_id}`) && !selectedBoards.has(thread.board_id)) throw { code: 'STALE_TARGET', exit_code: 5, message: 'Select the deleted parent Board to restore this sample Thread.' }
          }
        }
        if (action === 'restore') {
          for (const item of normalized.filter((target) => target.kind === 'board')) deletedAt.delete(`board:${item.id}`)
          for (const item of normalized.filter((target) => target.kind === 'thread')) deletedAt.delete(`thread:${item.id}`)
        } else {
          for (const item of normalized.filter((target) => target.kind === 'board')) purgeSampleConversation('board', item.id)
          for (const item of normalized.filter((target) => target.kind === 'thread')) if (threads.some((thread) => thread.id === item.id)) purgeSampleConversation('thread', item.id)
        }
        return { action, items: normalized }
      }
      if (operation === 'display.settings.set') {
        if (!['en', 'ja'].includes(String(manageArgs.language)) || !['light', 'dark'].includes(String(manageArgs.theme))) throw new Error('Invalid sample display settings')
        sampleDisplaySettings = manageArgs as import('./types').DisplaySettings
        return { ...sampleDisplaySettings }
      }
      if (operation === 'notification.settings.set') {
        sampleNotificationSettings = { enabled: manageArgs.enabled === true, include_excerpt: manageArgs.include_excerpt === true }
        return { ...sampleNotificationSettings }
      }
      if (operation === 'thread.subscribe') {
        const id = String(manageArgs.id), subscribed = manageArgs.subscribed === true
        if (subscribed) sampleSubscriptions.add(id)
        else sampleSubscriptions.delete(id)
        return { id, subscribed }
      }
      if (operation === 'post.read.mark') {
        const ids = manageArgs.post_ids as string[]
        const allowed = new Set(visiblePosts(String(manageArgs.thread_id)).filter((post) => !post.deleted && post.body !== null).map((post) => post.id))
        if (!ids.length || ids.length > 200 || ids.some((id) => !allowed.has(id))) throw { code: 'INVALID_INPUT', exit_code: 2, message: 'Invalid displayed posts.' }
        ids.forEach((id) => sampleReadPosts.add(id))
        return { marked: new Set(ids).size }
      }
      if (operation === 'thread.pin') {
        const id = String(manageArgs.id)
        if (!visibleThreads().some((thread) => thread.id === id)) throw new Error('Sample Thread was not found.')
        const next = new Set(pinnedIds)
        const pinned = manageArgs.pinned === true
        if (pinned) next.add(id)
        else next.delete(id)
        localStorage.setItem(pinStorageKey, JSON.stringify([...next]))
        pinnedIds = next
        return { id, pinned }
      }
      if (operation.startsWith('post.')) {
        const post = postByNo(String(manageArgs.thread_id), Number(manageArgs.no))
        if (!post) throw new Error('Sample Post was not found.')
        post.deleted = operation === 'post.delete'
        post.deleted_at = post.deleted ? new Date().toISOString() : null
        if (post.deleted) {
          if (post.body !== null) deletedBodies.set(post.id, post.body)
          post.body = null
        } else {
          post.body = deletedBodies.get(post.id) ?? null
          deletedBodies.delete(post.id)
        }
        recordActivity(operation, post)
        return post
      }
      const kind = operation.startsWith('board.') ? 'board' : 'thread'
      const id = String(manageArgs.id)
      const key = `${kind}:${id}`
      if (operation.endsWith('.purge')) {
        if (manageArgs.confirmed !== true || !deletedAt.has(key) || !(kind === 'board' ? boards.some((board) => board.id === id) : threads.some((thread) => thread.id === id))) throw { code: 'STALE_TARGET', exit_code: 5, message: 'The sample conversation is no longer available for permanent deletion.' }
        purgeSampleConversation(kind, id)
        return { id, deleted: true, purged: true }
      }
      if (operation.endsWith('.delete')) deletedAt.set(key, new Date().toISOString())
      else deletedAt.delete(key)
      const target = kind === 'board' ? boards.find((item) => item.id === id) : threads.find((item) => item.id === id)
      if (target) recordActivity(operation, target)
      return { id, deleted: deletedAt.has(key) }
    }
    case 'gui_edit': {
      const editArgs = (request.args || {}) as { thread_id: string; no: number; body: string; if_revision: number }
      const post = postByNo(editArgs.thread_id, editArgs.no)
      if (!post) throw new Error('Sample Post was not found.')
      if (post.revision !== editArgs.if_revision) throw new Error('Sample revision changed.')
      if (post.body !== null) histories.set(`${post.thread_id}:${post.no}`, [...(histories.get(`${post.thread_id}:${post.no}`) || []), { revision: post.revision, body: post.body, saved_at: new Date().toISOString(), editor_id: sampleAccount.id, editor_display_id: sampleAccount.name, editor_model_name: 'User' }])
      post.body = editArgs.body
      post.revision += 1
      post.updated_at = new Date().toISOString()
      post.edited_by = sampleAccount.id
      post.editor_display_id = sampleAccount.name
      post.editor_model_name = 'User'
      recordActivity('post.edit', post)
      return post
    }
    case 'gui_create': {
      const operation = request.operation as string
      if (operation === 'board.create') {
        const board: Board = { id: `sample-board-${++entityNumber}`, name: String(args.name || 'New Board'), description: String(args.description || ''), created_at: new Date().toISOString() }
        boards.push(board)
        capabilities.boards.push({ id: board.id, thread_create: true, board_delete: true })
        recordActivity(operation, board)
        return board
      }
      if (operation === 'thread.create') {
        const thread: Thread = { id: `sample-thread-${++entityNumber}`, board_id: String(args.board_id || ''), name: String(args.name || 'New Thread'), description: String(args.description || ''), created_at: new Date().toISOString() }
        threads.push(thread)
        capabilities.threads.push({ id: thread.id, post_create: true, response_create: true, post_edit: true, post_history: true, thread_delete: true, post_delete: true, post_restore: true })
        recordActivity(operation, thread)
        return thread
      }
      if (operation === 'post.create' || operation === 'response.create') return makePost(String(args.thread_id), String(args.body || ''), operation === 'response.create' ? Number(args.to) : null)
      throw new Error(`Preview does not support create operation: ${operation}`)
    }
    case 'gui_ack_create': return creationStatus
    case 'gui_retry_create': throw new Error('No preview creation is pending.')
    case 'gui_discard_create': return creationStatus
    case 'gui_rules_check': {
      // A canned fixture for exercising UI states; real policy checks run in Rust.
      if (args.project_id !== 'project-preview' || args.actor_id !== 'ai-preview') throw { code: 'DENIED', exit_code: 3, message: 'Sample diagnostic subject is unavailable.' }
      const operation = String(args.operation)
      if (['changes.get', 'changes.wait'].includes(operation)) return { allowed: false, reason: 'not_supported' }
      if (operation === 'project.register') return { allowed: false, reason: 'not_allowed' }
      if (operation === 'rules.validate') return { allowed: false, reason: 'operation_prohibited' }
      if (operation.endsWith('.delete') || operation.endsWith('.restore')) return { allowed: false, reason: 'operation_prohibited' }
      if (operation === 'post.edit' || operation === 'thread.edit' || operation === 'board.edit') return { allowed: false, reason: 'owner_only' }
      if (operation.endsWith('.create')) return { allowed: false, reason: 'not_allowed' }
      if (sampleRulesActions.includes(operation)) return { allowed: true, reason: 'allowed' }
      return { allowed: false, reason: 'not_supported' }
    }
    case 'gui_rules_manage': {
      const result = previewRulesValidation(args.source)
      if (request.operation === 'rules.validate') return result
      if (request.operation !== 'rules.save' || !result.valid) throw { code: 'RULES_INVALID', exit_code: 4, message: 'Sample rules are invalid.' }
      const key = typeof args.project_id === 'string' ? args.project_id : ''
      const current = previewRulesDocument(args.project_id)
      if (sampleRulesConflict) {
        sampleRulesConflict = false
        sampleRules.set(key, { source: current.source + '\nExternal preview edit.\n', revision: `preview-${++sampleRulesRevision}` })
        throw { code: 'RULES_CHANGED', exit_code: 4, message: 'The sample rules changed externally. Your draft has been retained.' }
      }
      if (args.if_revision !== current.revision) throw { code: 'RULES_CHANGED', exit_code: 4, message: 'The sample rules changed. Reload before saving.' }
      sampleRules.set(key, { source: String(args.source), revision: `preview-${++sampleRulesRevision}` })
      return previewRulesDocument(args.project_id)
    }
    case 'gui_authorization_manage': {
      if (request.operation === 'role.create') {
        const name = typeof args.name === 'string' ? args.name.trim() : ''
        if (!name) throw new Error('A role name is required.')
        if (sampleRoles.some((role) => role.name === name)) throw new Error('A role with this name already exists.')
        const role = { id: `sample-role-${++entityNumber}`, name, created_at: now }
        sampleRoles.push(role)
        return { ...role }
      }
      if (request.operation === 'role.assign') {
        const account = sampleAccounts.find((item) => item.id === args.account_id)
        const role = sampleRoles.find((item) => item.id === args.role_id)
        if (!account || account.is_admin || !role || typeof args.assigned !== 'boolean') throw new Error('Invalid sample role assignment.')
        account.role_ids = account.role_ids.filter((id) => id !== role.id)
        if (args.assigned) account.role_ids.push(role.id)
        return { account_id: account.id, role_id: role.id, assigned: args.assigned }
      }
      if (request.operation !== 'permission.set') throw new Error('Unsupported sample authorization operation.')
      const catalog = samplePermissionOperations.find((item) => item.operation === args.operation)
      const subjectExists = args.subject_type === 'account' ? sampleAccounts.some((item) => item.id === args.subject_id && !item.is_admin) : args.subject_type === 'role' && sampleRoles.some((item) => item.id === args.subject_id)
      if (!subjectExists || !catalog || typeof args.scope !== 'string' || !catalog.scopes.includes(args.scope) || typeof args.target_id !== 'string' || !['allow', 'deny', 'inherit'].includes(String(args.decision))) throw new Error('Invalid sample permission.')
      if (args.scope === 'global' ? args.target_id !== '' : !args.target_id.trim()) throw new Error('Invalid sample permission target.')
      const key = `${args.subject_type}:${args.subject_id}`
      const items = (samplePermissions.get(key) || []).filter((item) => !(item.operation === args.operation && item.scope === args.scope && item.target_id === args.target_id))
      if (args.decision !== 'inherit') items.push({ operation: String(args.operation), scope: args.scope, target_id: args.target_id, decision: String(args.decision) })
      samplePermissions.set(key, items)
      return { ...args }
    }
    case 'gui_reveal_recovery_code':
      if (!snapshot.account?.is_admin || snapshot.pendingIssue || creationStatus.pending) throw new Error('The preview account cannot display a recovery code.')
      return { recovery_code: 'PREVIEW-ONLY-NOT-A-RECOVERY-CODE' }
    case 'gui_cancel_pending':
      snapshot.pendingIssue = false
      return { ...snapshot }
    case 'gui_logout':
      snapshot.account = null
      return { ...snapshot }
    case 'gui_recover_account': throw new Error('Authentication is disabled in the GUI preview.')
    case 'gui_authenticate':
      if (setupSample && snapshot.setupRequired && request.mode === 'setup' && typeof request.name === 'string' && request.name.trim() && typeof request.password === 'string') {
        snapshot.account = { ...sampleAccount, name: request.name.trim(), lock_enabled: request.password !== '' }
        snapshot.setupRequired = false
        return { ...snapshot }
      }
      throw new Error('Authentication is disabled in the GUI preview.')
    case 'gui_retry_pending': throw new Error('Authentication is disabled in the GUI preview.')
    default: throw new Error(`Preview IPC is closed to command: ${command}`)
  }
})

import App from './App'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode><App /></React.StrictMode>,
)
