// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Workspace from './Workspace'
import { guiAckCreate, guiCreate, guiCreationStatus, guiDiscardCreate, guiEdit, guiManage, guiManageConversationBatch, guiNotificationAck, guiNotificationActivation, guiOpenLink, guiPinThread, guiPreferenceGet, guiPreferenceSet, guiRead, guiRetryCreate } from './bridge'
import { flushRegisteredPreferences } from './nativeClosePreferences'
import { threadScrollPreferenceKey, writeThreadScrollPreference } from './threadScrollPreferences'
import { conversationViewPreferenceKey } from './conversationViewPreferences'
import type { ActivityItem, Board, ConversationCapabilities, Post, SearchItem, Thread } from './types'

vi.mock('./bridge', () => ({ guiRead: vi.fn(), guiCreate: vi.fn(), guiEdit: vi.fn(), guiManage: vi.fn(), guiManageConversationBatch: vi.fn(), guiSubscribeThread: vi.fn(), guiMarkDisplayedPostsRead: vi.fn(), guiNotificationActivation: vi.fn(async () => ({ state: 'none' })), guiNotificationAck: vi.fn(), guiOpenLink: vi.fn(), guiPinThread: vi.fn(), guiCreationStatus: vi.fn(), guiDiscardCreate: vi.fn(), guiRetryCreate: vi.fn(), guiAckCreate: vi.fn(), guiPreferenceGet: vi.fn(), guiPreferenceSet: vi.fn() }))

const board: Board = { id: 'board-a', name: 'Engineering', description: '', created_at: '2026-10-01T00:00:00Z' }
const threadA: Thread = { id: 'thread-a', board_id: board.id, name: 'Release review', description: 'Review notes', created_at: '2026-10-01T00:00:00Z' }
const threadPrivate: Thread = { id: 'thread-private', board_id: null, name: 'Private thread', description: '', created_at: '2026-10-01T00:00:00Z' }
const memberCapabilities: ConversationCapabilities = { board_create: false, boards: [{ id: board.id, thread_create: false, board_delete: false }], threads: [{ id: threadA.id, post_create: false, response_create: false, post_edit: false, post_history: false, thread_delete: false, post_delete: false, post_restore: false }, { id: threadPrivate.id, post_create: false, response_create: false, post_edit: false, post_history: false, thread_delete: false, post_delete: false, post_restore: false }] }
const editorCapabilities: ConversationCapabilities = { board_create: true, boards: [{ id: board.id, thread_create: true, board_delete: true }], threads: [{ id: threadA.id, post_create: true, response_create: true, post_edit: true, post_history: true, thread_delete: true, post_delete: true, post_restore: true }, { id: threadPrivate.id, post_create: true, response_create: true, post_edit: true, post_history: true, thread_delete: true, post_delete: true, post_restore: true }] }
function post(no: number, body: string | null, extra: Partial<Post> = {}): Post {
  return {
    id: `post-${no}`, thread_id: threadA.id, no, body,
    author: { id: 'author-a', model_name: 'Example Model', display_id: 'AB12', harness_name: 'Codex' },
    created_at: '2026-10-07T02:00:00Z', updated_at: '2026-10-07T02:00:00Z', revision: 1,
    reply_to: null, edited_by: null, editor_display_id: null, editor_model_name: null,
    deleted: false, deleted_at: null, response_count: 0, ...extra,
  }
}
function postPage(items: Post[], page = 1, pageSize = 50, totalPosts = items.length) {
  return { items, page, page_size: pageSize, total_posts: totalPosts, total_pages: Math.max(1, Math.ceil(totalPosts / pageSize)) }
}
function activity(id: number, action = 'post.create', extra: Partial<ActivityItem> = {}): ActivityItem {
  return {
    id, action, occurred_at: '2026-10-08T03:04:05Z', thread_id: threadA.id, thread_name: threadA.name,
    board_id: board.id, board_name: board.name, post_no: id, ...extra,
  }
}

let host: HTMLDivElement
let root: Root
let restoreThreadScrollMetrics: (() => void) | null = null
let restoreRequestAnimationFrame: (() => void) | null = null
let restoreThreadRects: (() => void) | null = null

async function settle() { await new Promise<void>((resolve) => setTimeout(resolve, 0)) }
function mockThreadScrollMetrics(scrollHeight = 1200, clientHeight = 300) {
  const originalHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollHeight')
  const originalClientHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight')
  Object.defineProperty(HTMLElement.prototype, 'scrollHeight', {
    configurable: true,
    get() { return this.getAttribute('data-testid') === 'thread-scroll-container' ? scrollHeight : originalHeight?.get?.call(this) ?? 0 },
  })
  Object.defineProperty(HTMLElement.prototype, 'clientHeight', {
    configurable: true,
    get() { return this.getAttribute('data-testid') === 'thread-scroll-container' ? clientHeight : originalClientHeight?.get?.call(this) ?? 0 },
  })
  if (typeof window.requestAnimationFrame === 'function') {
    const requestAnimationFrame = vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callback(0)
      return 1
    })
    restoreRequestAnimationFrame = () => {
      requestAnimationFrame.mockRestore()
      restoreRequestAnimationFrame = null
    }
  }
  restoreThreadScrollMetrics = () => {
    if (originalHeight) Object.defineProperty(HTMLElement.prototype, 'scrollHeight', originalHeight)
    else delete (HTMLElement.prototype as unknown as { scrollHeight?: number }).scrollHeight
    if (originalClientHeight) Object.defineProperty(HTMLElement.prototype, 'clientHeight', originalClientHeight)
    else delete (HTMLElement.prototype as unknown as { clientHeight?: number }).clientHeight
    restoreThreadScrollMetrics = null
  }
}
async function render(language: 'en' | 'ja' = 'en', accountId = 'test-account', refreshToken = 0) {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  await act(async () => { root.render(<Workspace language={language} accountId={accountId} refreshToken={refreshToken} onLogout={() => undefined} onSettings={() => undefined} busy={false} />); await settle() })
}
async function remount(language: 'en' | 'ja' = 'en', accountId = 'test-account', refreshToken = 0) {
  await act(async () => root.unmount())
  host.remove()
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  await act(async () => { root.render(<Workspace language={language} accountId={accountId} refreshToken={refreshToken} onLogout={() => undefined} onSettings={() => undefined} busy={false} />); await settle() })
}
async function rerender(language: 'en' | 'ja' = 'en', accountId = 'test-account', refreshToken = 0) {
  await act(async () => { root.render(<Workspace language={language} accountId={accountId} refreshToken={refreshToken} onLogout={() => undefined} onSettings={() => undefined} busy={false} />); await settle() })
}
function select(label: string) {
  const button = [...host.querySelectorAll('button')].find((item) => item.textContent === label)
  if (!button) throw new Error(`Thread button missing: ${label}`)
  button.click()
}
function fill(selector: string, value: string) {
  const input = host.querySelector<HTMLInputElement | HTMLTextAreaElement>(selector)
  if (!input) throw new Error(`Field missing: ${selector}`)
  act(() => {
    const prototype = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype
    Object.getOwnPropertyDescriptor(prototype, 'value')?.set?.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
}
function button(label: string) {
  const found = [...host.querySelectorAll('button')].find((item) => item.textContent?.trim() === label)
  if (!found) throw new Error(`Button missing: ${label}`)
  return found
}
function testButton(testId: string) {
  const found = host.querySelector<HTMLButtonElement>(`[data-testid="${testId}"]`)
  if (!found) throw new Error(`Button missing: ${testId}`)
  return found
}
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}
function searchItem(kind: SearchItem['kind'], id: string, extra: Partial<SearchItem> = {}): SearchItem {
  return {
    kind, id, board_id: board.id, thread_id: threadA.id, name: kind === 'board' || kind === 'thread' ? id : threadA.name,
    excerpt: `${kind} match`, post_no: kind === 'post' || kind === 'response' ? 87 : null,
    created_at: '2026-10-08T00:00:00Z', ...extra,
  }
}

beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(guiNotificationActivation).mockResolvedValue({ state: 'none' })
  vi.mocked(guiNotificationAck).mockResolvedValue(true)
  vi.mocked(guiPreferenceGet).mockImplementation(async (_accountId, { scope }) => ({
    value: scope === 'layout' ? { version: 1, treeWidth: 280, composerHeight: 230 } : scope === 'conversation_view' ? 'timeline' : null,
    migration_allowed: false,
  }))
  vi.mocked(guiPreferenceSet).mockResolvedValue({ saved: true })
  window.localStorage.clear()
  document.documentElement.style.overflow = ''
  document.body.style.overflow = ''
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
  vi.mocked(guiRead).mockImplementation(async (operation, args) => {
    if (operation === 'conversation.capabilities') return editorCapabilities as never
    if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
    if (operation === 'conversation.pins') return { items: [] } as never
    if (operation === 'conversation.subscriptions') return { items: [] } as never
    if (operation === 'conversation.unread') return { items: [], total: 0 } as never
    if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
    if (operation === 'board.list') return { items: [board] } as never
    if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
    if (operation === 'thread.get') return (args?.id === threadPrivate.id ? threadPrivate : threadA) as never
    if (operation === 'post.page') return postPage([post(1, 'Hello **world**'), post(2, 'Deleted body', { author: { id: 'unknown-author', model_name: '不明', display_id: 'CD34' }, deleted: true, deleted_at: '2026-10-07T03:00:00Z', response_count: 1 })]) as never
    if (operation === 'response.list') return { items: [post(3, 'Direct reply', { reply_to: 2 })], next_before_no: null } as never
    if (operation === 'post.get') return post(1, 'Parent post') as never
    if (operation === 'post.history') return { items: [] } as never
    throw new Error(`Unexpected read operation: ${operation}`)
  })
  vi.mocked(guiCreationStatus).mockResolvedValue({ pending: false, operation: null, expired: false })
  vi.mocked(guiCreate).mockResolvedValue({} as never)
  vi.mocked(guiOpenLink).mockResolvedValue(undefined)
  vi.mocked(guiEdit).mockResolvedValue({} as never)
  vi.mocked(guiManage).mockResolvedValue({} as never)
  vi.mocked(guiManageConversationBatch).mockResolvedValue({ action: 'restore', items: [] })
  vi.mocked(guiPinThread).mockImplementation(async (id, pinned) => ({ id, pinned }))
  vi.mocked(guiRetryCreate).mockResolvedValue({} as never)
  vi.mocked(guiAckCreate).mockResolvedValue({ pending: false, operation: null, expired: false })
  vi.mocked(guiDiscardCreate).mockResolvedValue({ pending: false, operation: null, expired: false })
})

afterEach(async () => {
  if (root) await act(async () => root.unmount())
  host?.remove()
  document.documentElement.style.overflow = ''
  document.body.style.overflow = ''
  restoreThreadScrollMetrics?.()
  restoreRequestAnimationFrame?.()
  restoreThreadRects?.()
})

describe('approved text limits', () => {
  it.each([['Create Board', 100], ['Create Thread', 200]] as const)('keeps an oversized %s draft and rejects submission before IPC', async (label, limit) => {
    await render()
    await act(async () => { button(label).click(); await settle() })
    fill('.creation-dialog .creation-form input', '𠮷'.repeat(limit + 1))
    const form = host.querySelector<HTMLFormElement>('.creation-dialog form')!
    await act(async () => { form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(guiCreate).not.toHaveBeenCalled()
    expect(host.querySelector<HTMLInputElement>('.creation-dialog input')?.value).toBe('𠮷'.repeat(limit + 1))
    expect(host.textContent).toContain(`${limit + 1} / ${limit} characters`)
    fill('.creation-dialog .creation-form input', '𠮷'.repeat(limit))
    expect(host.textContent).toContain(`${limit} / ${limit} characters`)
  })

  it('keeps an oversized body draft and blocks create and edit before IPC', async () => {
    await render()
    await act(async () => { select(threadA.name); await settle() })
    fill('.composer-panel textarea', '𠮷'.repeat(100_001))
    await act(async () => { host.querySelector<HTMLFormElement>('.composer-panel')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(guiCreate).not.toHaveBeenCalled()
    expect(host.textContent).toContain('100,001 / 100,000 characters')
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toHaveLength(200_002)
    await act(async () => { button('Edit').click(); await settle() })
    fill('.post-edit-panel textarea', 'a'.repeat(100_001))
    await act(async () => { host.querySelector<HTMLFormElement>('.post-edit-panel')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(guiEdit).not.toHaveBeenCalled()
    expect(host.querySelector<HTMLTextAreaElement>('.post-edit-panel textarea')?.value).toHaveLength(100_001)
  })
})

describe('past Threads', () => {
  it('moves a full Thread out of Boards, keeps it readable in Archive and localizes the tab', async () => {
    const original = vi.mocked(guiRead).getMockImplementation()!
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'thread.list') return { items: [{ ...threadA, archived: true }, threadPrivate] } as never
      if (operation === 'thread.get') return { ...threadA, archived: true } as never
      return original(operation, args) as never
    })
    await render()
    const boardsPanel = host.querySelector('[data-testid="workspace-panel-boards"]')!
    expect(boardsPanel.textContent).not.toContain(threadA.name)
    await act(async () => { testButton('workspace-tab-archive').click(); await settle() })
    const archive = host.querySelector('[data-testid="workspace-panel-archive"]')!
    expect(archive.textContent).toContain(threadA.name)
    expect(archive.textContent).toContain(board.name)
    expect(testButton('workspace-tab-boards').textContent).toBe('Boards')
    expect(testButton('workspace-tab-archive').textContent).toBe('Archive')
    expect(host.querySelector('[data-testid="workspace-counts"]')?.textContent).toContain('Boards: 1Threads: 1Archive: 1')
    await act(async () => { archive.querySelector<HTMLButtonElement>('button')!.click(); await settle() })
    expect(host.querySelector('h1')?.textContent).toBe(threadA.name)
    expect(host.textContent).toContain('1,000 Post limit')
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.disabled ?? true).toBe(true)
    expect(guiCreate).not.toHaveBeenCalled()
    await rerender('ja')
    expect(testButton('workspace-tab-archive').textContent).toBe('過去スレ')
    expect(testButton('workspace-tab-archive').previousElementSibling?.getAttribute('data-testid')).toBe('workspace-tab-pinned')
  })
  it('moves the selected Thread on refresh when its final number has been allocated', async () => {
    const original = vi.mocked(guiRead).getMockImplementation()!
    let archived = false
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'thread.list') return { items: [{ ...threadA, archived }, threadPrivate] } as never
      if (operation === 'thread.get') return { ...threadA, archived } as never
      return original(operation, args) as never
    })
    await render()
    await act(async () => { select(threadA.name); await settle() })
    archived = true
    await act(async () => { button('Refresh All Threads').click(); await settle() })
    expect(testButton('workspace-tab-archive').getAttribute('aria-selected')).toBe('true')
    expect(host.querySelector('[data-testid="workspace-panel-boards"]')?.textContent).not.toContain(threadA.name)
    expect(host.querySelector('h1')?.textContent).toBe(threadA.name)
  })
})

describe('notification click navigation', () => {
  it('marks a Board with a red dot for readable unread Threads while preserving Thread counts', async () => {
    const originalRead = vi.mocked(guiRead).getMockImplementation()!
    let count = 3
    let archived = false
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'thread.list') return { items: [{ ...threadA, archived }, threadPrivate] } as never
      if (operation === 'conversation.unread') return { items: [{ thread_id: threadA.id, unread_count: count }, { thread_id: 'hidden-thread', unread_count: 10 }], total: count + 10 } as never
      return originalRead(operation, args) as never
    })
    await render()
    expect(host.querySelector('.tree-board-toggle .unread-dot')?.getAttribute('aria-label')).toBe('3 unread posts')
    expect(host.querySelector('.tree-thread .unread-badge')?.textContent).toBe('3')
    await act(async () => { host.querySelector<HTMLButtonElement>('.tree-board-toggle')!.click(); await settle() })
    expect(host.querySelector('.tree-board-toggle .unread-dot')).not.toBeNull()
    count = 0
    await act(async () => { root.render(<Workspace language="en" accountId="test-account" refreshToken={1} onLogout={() => undefined} onSettings={() => undefined} busy={false} />); await settle() })
    expect(host.querySelector('.tree-board-toggle .unread-dot')).toBeNull()
    count = 3
    archived = true
    await act(async () => { root.render(<Workspace language="en" accountId="test-account" refreshToken={2} onLogout={() => undefined} onSettings={() => undefined} busy={false} />); await settle() })
    expect(host.querySelector('.tree-board-toggle .unread-dot')).toBeNull()
    expect(host.querySelector('.tree-nested-empty')?.textContent).toContain('No Threads')
    await act(async () => { testButton('workspace-tab-archive').click(); await settle() })
    expect(host.querySelector('[data-testid="workspace-panel-archive"] .unread-badge')?.textContent).toBe('3')
  })
  it('opens only a target qualified for the current account and acknowledges after loading', async () => {
    vi.mocked(guiNotificationActivation).mockResolvedValue({ state: 'ready', event_id: 'click-1', account_id: 'test-account', thread: threadA })
    await render()
    await act(async () => { await settle() })
    expect(guiRead).toHaveBeenCalledWith('thread.get', { id: threadA.id })
    expect(host.querySelector('[data-testid="thread-scroll-container"]')?.textContent).toContain('Hello world')
    expect(guiNotificationAck).toHaveBeenCalledWith('click-1')
    expect(guiManage).not.toHaveBeenCalledWith('post.read.mark', expect.anything())
  })

  it('never navigates or acknowledges a target for a different current account', async () => {
    vi.mocked(guiNotificationActivation).mockResolvedValue({ state: 'ready', event_id: 'click-wrong', account_id: 'other-account', thread: threadA })
    await render()
    expect(guiRead).not.toHaveBeenCalledWith('thread.get', expect.anything())
    expect(guiNotificationAck).not.toHaveBeenCalled()
  })

  it('shows a generic account prompt without target metadata and retains the intent', async () => {
    vi.mocked(guiNotificationActivation).mockResolvedValue({ state: 'account_required', event_id: 'click-account' })
    await render('ja')
    expect(host.querySelector('[data-testid="notification-error"]')?.textContent).toContain('通知を受け取ったアカウント')
    expect(guiNotificationAck).not.toHaveBeenCalled()
    expect(guiRead).not.toHaveBeenCalledWith('thread.get', expect.anything())
  })

  it('retains a navigation failure for an explicit retry and revalidates permissions', async () => {
    const read = vi.mocked(guiRead).getMockImplementation()!
    let fail = true
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'post.page' && fail) throw new Error('temporary read failure')
      return read(operation, args)
    })
    vi.mocked(guiNotificationActivation).mockResolvedValue({ state: 'ready', event_id: 'click-retry', account_id: 'test-account', thread: threadA })
    await render()
    expect(guiNotificationAck).not.toHaveBeenCalled()
    expect(host.querySelector('[data-testid="notification-error"]')?.textContent).toContain('Could not open')
    fail = false
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="notification-retry"]')!.click(); await settle() })
    expect(guiNotificationAck).toHaveBeenCalledWith('click-retry')
    expect(guiRead).toHaveBeenCalledWith('thread.get', { id: threadA.id })
  })
})

describe('read-only conversation workspace', () => {
  it('replaces Quick start with completed update notes, while a new update stays below Quick start', async () => {
    await render()
    await act(async () => root.render(<Workspace language="en" busy={false} onLogout={() => undefined} updateNotes={{ version: '0.2.0', notes: 'Updated features' }} />))
    const empty = host.querySelector('[data-testid="thread-empty-state"]')!
    expect(empty.textContent).toContain('What’s new')
    expect(empty.querySelector('.quick-start-list')).toBeNull()
    expect(empty.querySelector('.workspace-empty-icon')).toBeNull()
    expect(empty.getAttribute('aria-labelledby')).toBe('update-notes-title')
    await act(async () => root.render(<Workspace language="en" busy={false} onLogout={() => undefined} availableUpdate={{ version: '0.3.0' }} updateNotes={{ version: '0.2.0', notes: 'Updated features' }} />))
    expect(empty.querySelector('.quick-start-list')).not.toBeNull()
    expect(empty.querySelector('.update-release-notes')).toBeNull()
    expect(empty.textContent).toContain('A new update is available.')
  })
  it('shows a branded Quick start Empty State without a composer before any Thread is selected', async () => {
    await render()
    const emptyState = host.querySelector<HTMLElement>('[data-testid="thread-empty-state"]')
    expect(emptyState).not.toBeNull()
    expect(emptyState?.textContent).toContain('Swarm')
    expect(emptyState?.textContent).toContain('Start a conversation in a few steps.')
    expect(emptyState?.querySelectorAll('.quick-start-list li')).toHaveLength(3)
    expect(emptyState?.querySelector('img[alt=""], svg[aria-hidden="true"]')).not.toBeNull()
    expect(host.querySelector('.composer-pane')).toBeNull()
    expect(host.querySelector('.composer-panel')).toBeNull()
    expect(host.querySelector('[data-testid="composer-height-separator"]')).toBeNull()
  })

  it('closes the selected Thread back to the same Empty State without deleting it', async () => {
    await render()
    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')).not.toBeNull()
    const close = host.querySelector<HTMLButtonElement>('button[aria-label="Close Thread"]')
    expect(close).not.toBeNull()
    await act(async () => { close?.click(); await settle() })
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')).toBeNull()
    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    expect(host.querySelector('.composer-pane')).toBeNull()
    expect(host.querySelector('.composer-panel')).toBeNull()
    expect(guiManage).not.toHaveBeenCalled()
    expect(host.textContent).toContain('Release review')
  })

  it('localizes the Thread close action in Japanese', async () => {
    await render('ja')
    await act(async () => { select('Release review'); await settle() })
    const close = host.querySelector<HTMLButtonElement>('button[aria-label="スレッドを閉じる"]')
    expect(close).not.toBeNull()
    await act(async () => { close?.click(); await settle() })
    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    expect(host.querySelector('.composer-pane')).toBeNull()
  })

  it('places Archive after Pinned in an accessible keyboard-navigable tablist', async () => {
    await render()
    const tabs = ['boards', 'pinned', 'archive', 'activity'].map((name) => host.querySelector<HTMLButtonElement>(`[data-testid="workspace-tab-${name}"]`)!)
    expect(host.querySelector('[role="tablist"]')?.getAttribute('aria-label')).toBe('Workspace')
    expect(tabs.map((tab) => tab.textContent?.trim())).toEqual(['Boards', 'Pinned', 'Archive', 'Activity'])
    expect(tabs.map((tab) => tab.getAttribute('aria-selected'))).toEqual(['true', 'false', 'false', 'false'])
    expect(tabs.map((tab) => tab.tabIndex)).toEqual([0, -1, -1, -1])
    expect(host.querySelector<HTMLElement>('[data-testid="workspace-panel-boards"]')?.hidden).toBe(false)
    expect(host.querySelector<HTMLElement>('[data-testid="workspace-panel-pinned"]')?.hidden).toBe(true)

    tabs[0].focus()
    await act(async () => { tabs[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true })); await settle() })
    expect(tabs[1].getAttribute('aria-selected')).toBe('true')
    expect(document.activeElement).toBe(tabs[1])
    await act(async () => { tabs[1].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true })); await settle() })
    expect(tabs[2].getAttribute('aria-selected')).toBe('true')
    await act(async () => { tabs[2].dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true })); await settle() })
    expect(tabs[0].getAttribute('aria-selected')).toBe('true')
    await act(async () => { tabs[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })); await settle() })
    expect(tabs[3].getAttribute('aria-selected')).toBe('true')
    expect(host.querySelector<HTMLElement>('[data-testid="workspace-panel-activity"]')?.hidden).toBe(false)
  })

  it('pins and unpins through the server and keeps the selected Thread across workspace tabs', async () => {
    let serverPins: Thread[] = [threadA]
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: serverPins } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return (args?.id === threadPrivate.id ? threadPrivate : threadA) as never
      if (operation === 'post.page') return postPage([post(1, 'Pinned Thread body')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    vi.mocked(guiPinThread).mockImplementation(async (id, pinned) => {
      serverPins = pinned ? [threadA] : []
      return { id, pinned }
    })
    await render()
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-tab-pinned"]')?.click(); await settle() })
    const pinnedPanel = host.querySelector<HTMLElement>('[data-testid="workspace-panel-pinned"]')!
    expect(pinnedPanel.textContent).toContain('Release review')
    const pinnedRow = [...pinnedPanel.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent?.trim() === 'Release review')
    await act(async () => { pinnedRow?.click(); await settle() })
    expect(host.querySelector('.thread-detail')?.getAttribute('aria-label')).toBe('Release review')
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-tab-activity"]')?.click(); await settle() })
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-tab-boards"]')?.click(); await settle() })
    expect(host.querySelector('.thread-detail')?.getAttribute('aria-label')).toBe('Release review')
    expect(host.querySelector('.tree-thread.is-selected')?.getAttribute('aria-current')).toBe('page')

    const pinButton = host.querySelector<HTMLButtonElement>('[data-testid="thread-pin-button"]')!
    expect(pinButton.getAttribute('aria-pressed')).toBe('true')
    expect(pinButton.getAttribute('aria-label')).toBe('Unpin Thread')
    await act(async () => { pinButton.click(); await settle() })
    expect(guiPinThread).toHaveBeenCalledWith(threadA.id, false)
    expect(serverPins).toEqual([])
    expect(host.querySelector<HTMLButtonElement>('[data-testid="thread-pin-button"]')?.getAttribute('aria-pressed')).toBe('false')
    expect(Object.keys(window.localStorage).some((key) => /pin|activity/i.test(key))).toBe(false)
  })

  it('keeps Thread pin state isolated when a delayed pin result arrives after switching Threads', async () => {
    let resolvePin!: (result: { id: string; pinned: boolean }) => void
    let serverPins: Thread[] = []
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: serverPins } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return (args?.id === threadPrivate.id ? threadPrivate : threadA) as never
      if (operation === 'post.page') return postPage([post(1, 'Thread body', { thread_id: args?.thread_id as string })]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    vi.mocked(guiPinThread).mockReturnValueOnce(new Promise((resolve) => { resolvePin = resolve }))
    await render()
    await act(async () => { select('Release review'); await settle() })
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-pin-button"]')?.click() })
    await act(async () => { select('Private thread'); await settle() })
    expect(host.querySelector('.thread-detail')?.getAttribute('aria-label')).toBe('Private thread')
    await act(async () => { resolvePin({ id: threadA.id, pinned: true }); await settle() })
    expect(host.querySelector('.thread-detail')?.getAttribute('aria-label')).toBe('Private thread')
    expect(host.querySelector<HTMLButtonElement>('[data-testid="thread-pin-button"]')?.getAttribute('aria-pressed')).toBe('false')
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-tab-pinned"]')?.click(); await settle() })
    expect(host.querySelector('[data-testid="workspace-panel-pinned"]')?.textContent).toContain('Release review')
  })

  it('shows pin mutation errors without changing the selected Thread pin state', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return (args?.id === threadPrivate.id ? threadPrivate : threadA) as never
      if (operation === 'post.page') return postPage([post(1, 'Pin error fixture')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    vi.mocked(guiPinThread).mockRejectedValueOnce(new Error('pin write failed'))
    await render()
    await act(async () => { select('Release review'); await settle() })
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-pin-button"]')?.click(); await settle() })
    expect(host.querySelector('[data-testid="thread-pin-error"]')?.textContent).toContain('Could not save the Thread pin')
    expect(host.querySelector<HTMLButtonElement>('[data-testid="thread-pin-button"]')?.getAttribute('aria-pressed')).toBe('false')
    expect(host.querySelector('.thread-detail')?.getAttribute('aria-label')).toBe('Release review')
  })

  it('renders Activity metadata without Post bodies and follows deduplicated cursor pages', async () => {
    const first = [
      activity(100, 'post.create', { post_no: 7 }),
      activity(99, 'post.edit', { post_no: 6 }),
    ]
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') {
        if (args?.before_id === 99) return { items: [activity(99, 'post.edit', { post_no: 6 }), activity(98, 'response.create', { post_no: 5 })], next_before_id: 98 } as never
        if (args?.before_id === 98) return { items: [activity(97, 'thread.delete', { thread_name: 'Archived thread', post_no: null })], next_before_id: null } as never
        return { items: first, next_before_id: 99 } as never
      }
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return (args?.id === threadPrivate.id ? threadPrivate : threadA) as never
      if (operation === 'post.page') return postPage([post(1, 'must never appear in Activity')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-tab-activity"]')?.click(); await settle() })

    const list = host.querySelector<HTMLOListElement>('[data-testid="workspace-activity-list"]')!
    expect(list.querySelectorAll('li')).toHaveLength(2)
    expect(list.textContent).toContain('posted')
    expect(list.textContent).toContain('edited')
    expect(list.textContent).toContain('Engineering')
    expect(list.textContent).toContain('Release review')
    expect(list.textContent).toContain('No. 7')
    expect(list.querySelector('time')?.getAttribute('dateTime')).toBe('2026-10-08T03:04:05Z')
    expect(list.textContent).not.toContain('must never appear in Activity')
    expect(list.querySelector('.timeline-post, .post-body')).toBeNull()

    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-activity-load-more"]')?.click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('conversation.activity', { limit: 50, before_id: 99 })
    expect(host.querySelectorAll('[data-testid="workspace-activity-list"] li')).toHaveLength(3)
    expect(host.querySelector('[data-testid="workspace-activity-list"]')?.textContent).toContain('replied')

    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-activity-load-more"]')?.click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('conversation.activity', { limit: 50, before_id: 98 })
    expect(host.querySelectorAll('[data-testid="workspace-activity-list"] li')).toHaveLength(4)
    expect(host.querySelector('[data-testid="workspace-activity-list"]')?.textContent).toContain('Archived thread')
    expect(host.querySelector('[data-testid="workspace-activity-load-more"]')).toBeNull()
  })

  it('continues through an empty Activity cursor page to metadata on the next page', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') {
        if (args?.before_id === 20) return { items: [activity(19, 'thread.create', { thread_name: 'Visible after filtered records', post_no: null })], next_before_id: null } as never
        return { items: [], next_before_id: 20 } as never
      }
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return threadA as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-tab-activity"]')?.click(); await settle() })
    expect(host.querySelector('[data-testid="workspace-activity-list"]')).toBeNull()
    expect(host.querySelector('[data-testid="workspace-activity-load-more"]')).not.toBeNull()
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-activity-load-more"]')?.click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('conversation.activity', { limit: 50, before_id: 20 })
    expect(host.querySelector('[data-testid="workspace-activity-list"]')?.textContent).toContain('Visible after filtered records')
    expect(host.querySelector('[data-testid="workspace-activity-load-more"]')).toBeNull()
  })

  it('allows retry after Activity read and load-more failures without losing loaded rows', async () => {
    let initialReads = 0
    let failMore = true
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') {
        if (args?.before_id !== undefined) {
          if (failMore) throw new Error('cursor temporarily unavailable')
          return { items: [activity(9, 'post.delete')], next_before_id: null } as never
        }
        initialReads += 1
        if (initialReads === 1) throw new Error('activity temporarily unavailable')
        return { items: [activity(10)], next_before_id: 9 } as never
      }
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return threadA as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-tab-activity"]')?.click(); await settle() })
    expect(host.querySelector('[data-testid="workspace-activity-error"]')?.textContent).toContain('Could not load Activity')
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-activity-retry"]')?.click(); await settle() })
    expect(host.querySelectorAll('[data-testid="workspace-activity-list"] li')).toHaveLength(1)
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-activity-load-more"]')?.click(); await settle() })
    expect(host.querySelector('[data-testid="workspace-activity-error"]')?.textContent).toContain('Could not load Activity')
    expect(host.querySelectorAll('[data-testid="workspace-activity-list"] li')).toHaveLength(1)
    failMore = false
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-activity-retry"]')?.click(); await settle() })
    expect(host.querySelector('[data-testid="workspace-activity-error"]')).toBeNull()
    expect(host.querySelectorAll('[data-testid="workspace-activity-list"] li')).toHaveLength(1)
    expect(host.querySelector('[data-testid="workspace-activity-load-more"]')).not.toBeNull()
  })

  it('ignores a delayed Activity result after the active account changes', async () => {
    let resolveAccountA!: (result: { items: ActivityItem[]; next_before_id: number | null }) => void
    let activityReadCount = 0
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') {
        activityReadCount += 1
        if (activityReadCount > 1) return { items: [activity(2, 'thread.create', { thread_name: 'Account B activity', post_no: null })], next_before_id: null } as never
        return new Promise((resolve) => { resolveAccountA = resolve }) as never
      }
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return threadA as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render('en', 'account-a')
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-tab-activity"]')?.click() })
    expect(host.querySelector('[data-testid="workspace-activity-loading"]')).not.toBeNull()
    await act(async () => { await rerender('en', 'account-b'); await settle() })
    expect(host.querySelector('[data-testid="workspace-activity-list"]')?.textContent).toContain('Account B activity')
    await act(async () => { resolveAccountA({ items: [activity(1, 'thread.create', { thread_name: 'Stale Account A activity', post_no: null })], next_before_id: null }); await settle() })
    expect(host.querySelector('[data-testid="workspace-activity-list"]')?.textContent).toContain('Account B activity')
    expect(host.querySelector('[data-testid="workspace-activity-list"]')?.textContent).not.toContain('Stale Account A activity')
  })

  it('ignores an Activity result after leaving and reopening the Activity tab', async () => {
    let resolveFirstRead!: (result: { items: ActivityItem[]; next_before_id: number | null }) => void
    let activityReadCount = 0
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') {
        activityReadCount += 1
        if (activityReadCount === 1) return new Promise((resolve) => { resolveFirstRead = resolve }) as never
        return { items: [activity(22, 'thread.create', { thread_name: 'Fresh Activity result', post_no: null })], next_before_id: null } as never
      }
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return threadA as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-tab-activity"]')?.click() })
    expect(host.querySelector('[data-testid="workspace-activity-loading"]')).not.toBeNull()
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-tab-boards"]')?.click(); await settle() })
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-tab-activity"]')?.click(); await settle() })
    expect(host.querySelector('[data-testid="workspace-activity-list"]')?.textContent).toContain('Fresh Activity result')
    await act(async () => { resolveFirstRead({ items: [activity(21, 'thread.create', { thread_name: 'Stale Activity result', post_no: null })], next_before_id: null }); await settle() })
    expect(host.querySelector('[data-testid="workspace-activity-list"]')?.textContent).toContain('Fresh Activity result')
    expect(host.querySelector('[data-testid="workspace-activity-list"]')?.textContent).not.toContain('Stale Activity result')
  })

  it('supports keyboard resizing and clamps the Board pane to its accessible bounds', async () => {
    await render()
    const separator = host.querySelector<HTMLElement>('[role="separator"][aria-orientation="vertical"][aria-label="Resize Boards pane"]')
    expect(separator).not.toBeNull()
    const minimum = Number(separator?.getAttribute('aria-valuemin'))
    const maximum = Number(separator?.getAttribute('aria-valuemax'))
    expect(minimum).toBe(220)
    expect(maximum).toBeGreaterThanOrEqual(minimum)
    expect(maximum).toBeLessThanOrEqual(480)
    separator?.focus()

    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true })) })
    expect(separator?.getAttribute('aria-valuenow')).toBe(String(minimum))
    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true })) })
    expect(separator?.getAttribute('aria-valuenow')).toBe(String(minimum))
    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true })) })
    expect(separator?.getAttribute('aria-valuenow')).toBe(String(minimum + 16))
    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })) })
    expect(separator?.getAttribute('aria-valuenow')).toBe(String(maximum))
    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true })) })
    expect(separator?.getAttribute('aria-valuenow')).toBe(String(maximum))
  })

  it('places Pin before the page controls, with Delete then Close rightmost', async () => {
    await render()
    await act(async () => { select('Release review'); await settle() })
    const actions = host.querySelector<HTMLElement>('.thread-heading-actions')!
    const controls = [...actions.querySelectorAll<HTMLButtonElement>('button')]
    const labels = controls.map((item) => item.getAttribute('aria-label') || item.textContent?.trim())
    expect(labels).toEqual(['Subscribe to Thread', 'Pin Thread', 'Page top', 'Page bottom', 'Delete', 'Close Thread'])
    expect(controls[5].textContent?.trim()).toBe('×')
  })

  it('jumps to the top and bottom of the current Post page without fetching another page', async () => {
    await render()
    await act(async () => { select('Release review'); await settle() })
    fill('.composer-panel textarea', 'Draft survives timeline navigation')
    const container = host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')!
    Object.defineProperties(container, {
      scrollHeight: { configurable: true, value: 800 },
      clientHeight: { configurable: true, value: 200 },
    })
    const postListCallsBefore = vi.mocked(guiRead).mock.calls.filter(([operation]) => operation === 'post.page').length
    container.scrollTop = 240
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-page-top-button"]')?.click() })
    expect(container.scrollTop).toBe(0)
    expect(vi.mocked(guiRead).mock.calls.filter(([operation]) => operation === 'post.page')).toHaveLength(postListCallsBefore)
    container.scrollTop = 100
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-page-bottom-button"]')?.click() })
    expect(container.scrollTop).toBe(600)
    expect(vi.mocked(guiRead).mock.calls.filter(([operation]) => operation === 'post.page')).toHaveLength(postListCallsBefore)
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toBe('Draft survives timeline navigation')
  })

  it('persists each Thread scroll position across switching, closing, and reopening the workspace', async () => {
    mockThreadScrollMetrics()
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return (args?.id === threadPrivate.id ? threadPrivate : threadA) as never
      if (operation === 'post.page' && args?.thread_id === threadPrivate.id) return postPage([post(1, 'Private body', { thread_id: threadPrivate.id })]) as never
      if (operation === 'post.page') return postPage([post(1, 'Scroll fixture body'), post(2, 'Second body')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    const accountId = 'scroll-account'
    await render('en', accountId)
    await act(async () => { select('Release review'); await settle() })
    let container = host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')!
    container.scrollTop = 315
    await act(async () => { container.dispatchEvent(new Event('scroll')); await settle() })
    const threadAKey = threadScrollPreferenceKey(accountId, threadA.id)
    const savedA = JSON.parse(window.localStorage.getItem(threadAKey)!) as { postNo: number | null; scrollTop: number; atBottom: boolean }
    expect(savedA.scrollTop).toBe(315)
    expect(savedA.atBottom).toBe(false)
    expect(window.localStorage.getItem(threadScrollPreferenceKey(accountId, threadPrivate.id))).toBeNull()
    expect(window.localStorage.getItem(threadScrollPreferenceKey('other-account', threadA.id))).toBeNull()
    expect(window.localStorage.getItem(threadAKey)).not.toContain('Scroll fixture body')

    await act(async () => { select('Private thread'); await settle() })
    container = host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')!
    container.scrollTop = 120
    await act(async () => { container.dispatchEvent(new Event('scroll')); await settle() })
    expect(JSON.parse(window.localStorage.getItem(threadScrollPreferenceKey(accountId, threadPrivate.id))!).scrollTop).toBe(120)

    await act(async () => { select('Release review'); await settle() })
    container = host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')!
    expect(container.scrollTop).toBe(315)
    await act(async () => { host.querySelector<HTMLButtonElement>('button[aria-label="Close Thread"]')?.click(); await settle() })
    await remount('en', accountId)
    await act(async () => { select('Release review'); await settle() })
    container = host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')!
    expect(container.scrollTop).toBe(315)
  })

  it('falls back to the latest page for corrupt saved Thread positions', async () => {
    const key = threadScrollPreferenceKey('corrupt-scroll-account', threadA.id)
    window.localStorage.setItem(key, '{not valid json')
    await render('en', 'corrupt-scroll-account')
    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector('#post-1')).not.toBeNull()
    expect(host.textContent).toContain('Hello world')
    expect(host.querySelector('[data-testid="thread-scroll-status"]')).toBeNull()
  })

  it('reports when Thread scroll position cannot be saved', async () => {
    mockThreadScrollMetrics()
    await render()
    await act(async () => { select('Release review'); await settle() })
    const setItem = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('storage is full') })
    const container = host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')!
    container.scrollTop = 180
    await act(async () => { container.dispatchEvent(new Event('scroll')); await settle() })
    const status = host.querySelector<HTMLElement>('[data-testid="thread-scroll-storage-status"]')
    expect(status?.textContent).toMatch(/could not be saved|保存できません/i)
    setItem.mockRestore()
  })

  it('discards a delayed saved-anchor read after the Thread is closed', async () => {
    let resolveAnchorPage!: (page: ReturnType<typeof postPage>) => void
    writeThreadScrollPreference('test-account', threadA.id, { version: 1, postNo: 4, offset: 12, scrollTop: 90, atBottom: false })
    vi.mocked(guiRead).mockImplementation((operation, args) => {
      if (operation === 'conversation.capabilities') return Promise.resolve(editorCapabilities) as never
      if (operation === 'conversation.deleted') return Promise.resolve({ boards: [], threads: [] }) as never
      if (operation === 'board.list') return Promise.resolve({ items: [board] }) as never
      if (operation === 'thread.list') return Promise.resolve({ items: [threadA, threadPrivate] }) as never
      if (operation === 'thread.get') return Promise.resolve(args?.id === threadPrivate.id ? threadPrivate : threadA) as never
      if (operation === 'post.page' && args?.thread_id === threadA.id && args.page === 1 && args.anchor_no === 4) {
        return new Promise((resolve) => { resolveAnchorPage = resolve }) as never
      }
      if (operation === 'post.page' && args?.thread_id === threadA.id) return Promise.resolve(postPage([post(51, 'Latest page body')], 2, 50, 51)) as never
      if (operation === 'post.page' && args?.thread_id === threadPrivate.id) return Promise.resolve(postPage([post(1, 'Private thread body', { thread_id: threadPrivate.id })])) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 1, page_size: 50, anchor_no: 4 })
    expect(host.textContent).toContain('Loading Thread')
    const key = threadScrollPreferenceKey('test-account', threadA.id)
    const savedBeforeRestore = window.localStorage.getItem(key)
    await act(async () => { host.querySelector<HTMLButtonElement>('button[aria-label="Close Thread"]')?.click(); await settle() })
    expect(window.localStorage.getItem(key)).toBe(savedBeforeRestore)
    await act(async () => { resolveAnchorPage(postPage([post(1, 'Stale saved-anchor body')])); await settle() })
    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    expect(host.textContent).not.toContain('Stale saved-anchor body')
    expect(host.querySelector('.composer-pane')).toBeNull()
  })

  it('restores a saved Post anchor with one server-selected page request', async () => {
    writeThreadScrollPreference('test-account', threadA.id, { version: 1, postNo: 4, offset: 12, scrollTop: 90, atBottom: false })
    const firstPage = Array.from({ length: 50 }, (_, index) => post(index + 1, `Page body ${index + 1}`))
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage(firstPage, 1, 50, 100) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 1, page_size: 50, anchor_no: 4 })
    expect(host.querySelector('#post-4')?.textContent).toContain('Page body 4')
    expect(host.querySelector('#post-50')).not.toBeNull()
    expect(host.querySelector('#post-51')).toBeNull()
    expect(vi.mocked(guiRead).mock.calls.filter(([operation]) => operation === 'post.page')).toHaveLength(1)
  })

  it('retries a missing saved Post anchor without revoking readable Thread access', async () => {
    writeThreadScrollPreference('test-account', threadA.id, { version: 1, postNo: 5, offset: -24, scrollTop: 480, atBottom: false, page: 1, pageSize: 50 })
    const remainingPosts = [1, 2, 3, 4].map((no) => post(no, `Remaining fixture Post ${no}`))
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return (args?.id === threadPrivate.id ? threadPrivate : threadA) as never
      if (operation === 'post.page' && args?.anchor_no === 5) throw { code: 'NOT_FOUND', exit_code: 5, message: 'Post 5 was not found.' }
      if (operation === 'post.page') return postPage(remainingPosts, 1, 50, 4) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })

    await render()
    await act(async () => { select('Release review'); await settle() })

    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 1, page_size: 50, anchor_no: 5 })
    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 1, page_size: 50 })
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')).not.toBeNull()
    expect(host.querySelector('.tree-thread.is-selected')?.getAttribute('aria-current')).toBe('page')
    expect(host.querySelector('.composer-panel textarea')).not.toBeNull()
    expect(host.textContent).toContain('Remaining fixture Post 4')
    expect(host.querySelector('[data-testid="thread-scroll-status"]')?.textContent).toContain('Could not restore the saved Thread position')
    expect(host.querySelector('[data-testid="thread-scroll-status"]')?.getAttribute('role')).toBe('alert')
    expect(host.querySelector('[data-testid="thread-empty-state"]')).toBeNull()
  })

  it('keeps the visible Post anchor when changing page size', async () => {
    mockThreadScrollMetrics()
    const firstPage = Array.from({ length: 50 }, (_, index) => post(index + 1, `Page body ${index + 1}`))
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page' && args?.page_size === 100) return postPage(Array.from({ length: 100 }, (_, index) => post(index + 1, `Expanded body ${index + 1}`)), 1, 100, 120) as never
      if (operation === 'post.page') return postPage(firstPage, 1, 50, 120) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    const container = host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')!
    container.scrollTop = 200
    vi.spyOn(container, 'getBoundingClientRect').mockReturnValue({ top: 0, bottom: 400, left: 0, right: 600, width: 600, height: 400, x: 0, y: 0, toJSON: () => ({}) } as DOMRect)
    vi.spyOn(container.querySelector<HTMLElement>('.thread-heading')!, 'getBoundingClientRect').mockReturnValue({ top: 0, bottom: 100, left: 0, right: 600, width: 600, height: 100, x: 0, y: 0, toJSON: () => ({}) } as DOMRect)
    const anchor = container.querySelector<HTMLElement>('#post-4')!
    vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({ top: 145, bottom: 195, left: 0, right: 600, width: 600, height: 50, x: 0, y: 145, toJSON: () => ({}) } as DOMRect)
    await act(async () => { container.dispatchEvent(new Event('scroll')); await settle() })
    const persisted = JSON.parse(window.localStorage.getItem(threadScrollPreferenceKey('test-account', threadA.id))!)
    expect(persisted.postNo).toBe(4)
    const sizeSelect = host.querySelector<HTMLSelectElement>('[data-testid="thread-page-size-select"]')!
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(sizeSelect, '100')
      sizeSelect.dispatchEvent(new Event('change', { bubbles: true }))
      await settle()
    })
    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 1, page_size: 100, anchor_no: 4 })
  })

  it('navigates numbered pages with first, previous, next, and last controls', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') {
        const page = Number(args?.page ?? 1)
        const pageSize = Number(args?.page_size ?? 50)
        const firstNo = (page - 1) * pageSize + 1
        return postPage([post(firstNo, `Page ${page} body`)], page, pageSize, 1000) as never
      }
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 1, page_size: 50 })
    expect(host.querySelector('[data-testid="thread-page-number-1"]')?.getAttribute('aria-current')).toBe('page')
    expect(host.querySelector('[data-testid="thread-page-number-20"]')).not.toBeNull()
    expect(host.querySelector('.thread-pagination')?.textContent).toContain('…')
    expect(host.querySelector<HTMLButtonElement>('[data-testid="thread-page-first-button"]')?.disabled).toBe(true)
    expect(host.querySelector<HTMLButtonElement>('[data-testid="thread-page-previous-button"]')?.disabled).toBe(true)

    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-page-number-20"]')?.click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 20, page_size: 50 })
    expect(host.querySelector('[data-testid="thread-page-number-20"]')?.getAttribute('aria-current')).toBe('page')
    expect(host.querySelector('#post-951')).not.toBeNull()
    expect(host.querySelector<HTMLButtonElement>('[data-testid="thread-page-next-button"]')?.disabled).toBe(true)
    expect(host.querySelector<HTMLButtonElement>('[data-testid="thread-page-last-button"]')?.disabled).toBe(true)

    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-page-previous-button"]')?.click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 19, page_size: 50 })
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-page-next-button"]')?.click(); await settle() })
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-page-first-button"]')?.click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 1, page_size: 50 })
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-page-last-button"]')?.click(); await settle() })
    expect(host.querySelector('[data-testid="thread-page-number-20"]')?.getAttribute('aria-current')).toBe('page')
  })

  it('supports all page sizes and restores the selected size and page for the same Thread', async () => {
    mockThreadScrollMetrics()
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return (args?.id === threadPrivate.id ? threadPrivate : threadA) as never
      if (operation === 'post.page') {
        const page = Number(args?.page ?? 1)
        const pageSize = Number(args?.page_size ?? 50)
        return postPage([post((page - 1) * pageSize + 1, `Page ${page} size ${pageSize}`)], page, pageSize, 2000) as never
      }
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    const accountId = 'pagination-account'
    await render('en', accountId)
    await act(async () => { select('Release review'); await settle() })
    const sizeSelect = () => host.querySelector<HTMLSelectElement>('[data-testid="thread-page-size-select"]')!
    expect([...sizeSelect().options].map((option) => Number(option.value))).toEqual([50, 100, 500, 1000])
    for (const size of [100, 500, 1000]) {
      await act(async () => {
        Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(sizeSelect(), String(size))
        sizeSelect().dispatchEvent(new Event('change', { bubbles: true }))
        await settle()
      })
      expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 1, page_size: size })
      expect(sizeSelect().value).toBe(String(size))
    }
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-page-number-2"]')?.click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 2, page_size: 1000 })
    expect(JSON.parse(window.localStorage.getItem(threadScrollPreferenceKey(accountId, threadA.id))!)).toMatchObject({ page: 2, pageSize: 1000 })

    await act(async () => { select('Private thread'); await settle() })
    expect(JSON.parse(window.localStorage.getItem(threadScrollPreferenceKey(accountId, threadA.id))!)).toMatchObject({ page: 2, pageSize: 1000 })
    await act(async () => { select('Release review'); await settle() })
    expect(guiRead).toHaveBeenLastCalledWith('post.page', { thread_id: threadA.id, page: 2, page_size: 1000 })
    await remount('en', accountId)
    await act(async () => { select('Release review'); await settle() })
    expect(guiRead).toHaveBeenLastCalledWith('post.page', { thread_id: threadA.id, page: 2, page_size: 1000 })
  })

  it('blocks new Posts and Responses at the 1,000 item cap while allowing a saved retry', async () => {
    vi.mocked(guiCreationStatus).mockResolvedValueOnce({ pending: true, operation: 'post.create', expired: false })
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Cap fixture', { response_count: 1 }), post(1000, null, { deleted: true })], 1, 50, 1000) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel [data-testid="thread-post-limit-status"]')?.value).toContain('1,000 Post limit')
    expect(host.querySelector('.composer-panel > .thread-post-limit')).toBeNull()
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.disabled).toBe(true)
    const reply = [...host.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent?.includes('Reply · No. 1'))
    expect(reply?.disabled).toBe(true)
    await act(async () => { host.querySelector<HTMLFormElement>('.composer-panel')?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })) })
    expect(guiCreate).not.toHaveBeenCalled()
    expect(button('Retry saved creation').disabled).toBe(false)
    await act(async () => { button('Retry saved creation').click(); await settle() })
    expect(guiRetryCreate).toHaveBeenCalledTimes(1)
  })

  it('supports keyboard resizing and clamps the Composer pane height', async () => {
    await render()
    await act(async () => { select('Release review'); await settle() })
    const separator = host.querySelector<HTMLElement>('[data-testid="composer-height-separator"]')
    expect(separator?.getAttribute('role')).toBe('separator')
    expect(separator?.getAttribute('aria-orientation')).toBe('horizontal')
    expect(separator?.getAttribute('aria-label')).toBe('Resize composer pane')
    const minimum = Number(separator?.getAttribute('aria-valuemin'))
    const maximum = Number(separator?.getAttribute('aria-valuemax'))
      expect(minimum).toBe(200)
    expect(maximum).toBeGreaterThanOrEqual(minimum)
    expect(maximum).toBeLessThanOrEqual(480)
    separator?.focus()

    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true })) })
    expect(separator?.getAttribute('aria-valuenow')).toBe(String(minimum))
    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true })) })
    expect(separator?.getAttribute('aria-valuenow')).toBe(String(minimum + 16))
    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })) })
    expect(separator?.getAttribute('aria-valuenow')).toBe(String(maximum))
    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true })) })
    expect(separator?.getAttribute('aria-valuenow')).toBe(String(maximum))
    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true })) })
    expect(separator?.getAttribute('aria-valuenow')).toBe(String(maximum - 16))
    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true })) })
    await act(async () => { separator?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true })) })
    expect(separator?.getAttribute('aria-valuenow')).toBe(String(minimum))
  })

  it('persists Board and Composer sizes across workspace close and reopen for the same account', async () => {
    await render('en', 'persisted-account')
    const treeSeparator = host.querySelector<HTMLElement>('[role="separator"][aria-orientation="vertical"]')!
    await act(async () => { treeSeparator.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })) })
    const savedTreeWidth = treeSeparator.getAttribute('aria-valuenow')
    await act(async () => { select('Release review'); await settle() })
    const composerSeparator = host.querySelector<HTMLElement>('[data-testid="composer-height-separator"]')!
    await act(async () => { composerSeparator.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })) })
    const savedComposerHeight = composerSeparator.getAttribute('aria-valuenow')
    await remount('en', 'persisted-account')
    const reopenedTreeSeparator = host.querySelector<HTMLElement>('[role="separator"][aria-orientation="vertical"]')!
    expect(reopenedTreeSeparator.getAttribute('aria-valuenow')).toBe(savedTreeWidth)
    await act(async () => { select('Release review'); await settle() })
    const reopenedComposerSeparator = host.querySelector<HTMLElement>('[data-testid="composer-height-separator"]')!
    expect(reopenedComposerSeparator.getAttribute('aria-valuenow')).toBe(savedComposerHeight)
  })

  it('isolates saved layout sizes between accounts', async () => {
    await render('en', 'account-a')
    const treeSeparatorA = host.querySelector<HTMLElement>('[role="separator"][aria-orientation="vertical"]')!
    await act(async () => { treeSeparatorA.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })) })
    const accountATreeWidth = treeSeparatorA.getAttribute('aria-valuenow')
    await act(async () => { select('Release review'); await settle() })
    const composerSeparatorA = host.querySelector<HTMLElement>('[data-testid="composer-height-separator"]')!
    await act(async () => { composerSeparatorA.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })) })
    const accountAComposerHeight = composerSeparatorA.getAttribute('aria-valuenow')

    await remount('en', 'account-b')
    const treeSeparatorB = host.querySelector<HTMLElement>('[role="separator"][aria-orientation="vertical"]')!
    expect(treeSeparatorB.getAttribute('aria-valuenow')).toBe('280')
    await act(async () => { select('Release review'); await settle() })
    const composerSeparatorB = host.querySelector<HTMLElement>('[data-testid="composer-height-separator"]')!
    expect(composerSeparatorB.getAttribute('aria-valuenow')).toBe('230')

    await remount('en', 'account-a')
    expect(host.querySelector<HTMLElement>('[role="separator"][aria-orientation="vertical"]')?.getAttribute('aria-valuenow')).toBe(accountATreeWidth)
    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector<HTMLElement>('[data-testid="composer-height-separator"]')?.getAttribute('aria-valuenow')).toBe(accountAComposerHeight)
  })

  it('defaults to a collapsed log and restores its saved state per account', async () => {
    await render()
    const toggle = host.querySelector<HTMLButtonElement>('[data-testid="workspace-log-toggle"]')!
    expect(toggle.getAttribute('aria-expanded')).toBe('false')
    expect(toggle.getAttribute('aria-label')).toBe('Expand log')
    expect(toggle.textContent).toBe('▸')
    expect(host.querySelector('.workspace-log-body')?.hasAttribute('hidden')).toBe(true)
    expect(host.querySelector('[data-testid="workspace-log-error-dot"]')).toBeNull()
    expect(host.querySelector('[data-testid="workspace-status-footer"]')?.classList.contains('is-collapsed')).toBe(true)
    await act(async () => { toggle.click(); await settle() })
    expect(host.querySelector('.workspace-log-body')?.hasAttribute('hidden')).toBe(false)
    await remount()
    expect(host.querySelector('[data-testid="workspace-log-toggle"]')?.getAttribute('aria-expanded')).toBe('true')
    await remount('en', 'other-account')
    expect(host.querySelector('[data-testid="workspace-log-toggle"]')?.getAttribute('aria-expanded')).toBe('false')
    await remount()
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="workspace-log-toggle"]')!.click(); await settle() })
    await remount()
    expect(host.querySelector('[data-testid="workspace-log-toggle"]')?.getAttribute('aria-expanded')).toBe('false')
  })

  it('reports when saving a resized pane fails and signals hidden errors until resolved', async () => {
    await render()
    const setItem = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('storage is full') })
    const separator = host.querySelector<HTMLElement>('[role="separator"][aria-orientation="vertical"]')!
    await act(async () => { separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })) })
    const status = host.querySelector<HTMLElement>('[data-testid="layout-preferences-status"]')
    expect(status?.getAttribute('role')).toMatch(/status|alert/)
    expect(status?.textContent).toMatch(/save|storage|保存/i)
    expect(status?.closest('[data-testid="workspace-status-footer"]')?.parentElement?.classList.contains('conversation-column')).toBe(true)
    const log = host.querySelector('[data-testid="workspace-operation-log"]')!
    const service = host.querySelector('[data-testid="workspace-service-status"]')!
    expect(service.compareDocumentPosition(status!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(status?.compareDocumentPosition(log)! & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(host.querySelector('.workspace-toolbar + .layout-preferences-status')).toBeNull()
    expect(host.querySelector('[data-testid="workspace-log-error-dot"]')?.getAttribute('aria-label')).toBe('Errors present')
    setItem.mockRestore()
    await act(async () => { separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true })); await settle() })
    expect(host.querySelector('[data-testid="layout-preferences-status"]')).toBeNull()
    expect(host.querySelector('[data-testid="workspace-log-error-dot"]')).toBeNull()
  })

  it('flushes a newer resize and log toggle behind an in-flight write before native close is acknowledged', async () => {
    const previousInternals = Object.getOwnPropertyDescriptor(window, '__TAURI_INTERNALS__')
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    let finishFirst!: (value: { saved: true }) => void
    const saved: unknown[] = []
    vi.mocked(guiPreferenceSet).mockImplementation(async (_accountId, args) => {
      saved.push(args.value)
      if (saved.length === 1) return new Promise((resolve) => { finishFirst = resolve })
      return { saved: true }
    })
    try {
      await render()
      const separator = host.querySelector<HTMLElement>('[data-testid="tree-width-separator"]')!
      await act(async () => { separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })) })
      await act(async () => { await new Promise((resolve) => setTimeout(resolve, 220)) })
      expect(saved).toHaveLength(1)
      await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="workspace-log-toggle"]')!.click())
      await act(async () => { separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true })) })
      await settle()

      const flushing = flushRegisteredPreferences()
      await Promise.resolve()
      expect(saved).toHaveLength(1)
      finishFirst({ saved: true })
      await flushing
      expect(saved).toHaveLength(2)
      expect(saved[1]).toMatchObject({ version: 1, treeWidth: 220, composerHeight: 230, logExpanded: true })
    } finally {
      if (previousInternals) Object.defineProperty(window, '__TAURI_INTERNALS__', previousInternals)
      else delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
    }
  })

  it('does not resend an acknowledged resize during close flush if the bridge later fails', async () => {
    const previousInternals = Object.getOwnPropertyDescriptor(window, '__TAURI_INTERNALS__')
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    const saved: unknown[] = []
    vi.mocked(guiPreferenceSet).mockImplementation(async (_accountId, args) => {
      saved.push(args.value)
      if (saved.length === 1) return { saved: true }
      throw new Error('bridge unavailable')
    })
    try {
      await render()
      const separator = host.querySelector<HTMLElement>('[data-testid="tree-width-separator"]')!
      await act(async () => { separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })) })
      await act(async () => { await new Promise((resolve) => setTimeout(resolve, 220)) })
      expect(saved).toHaveLength(1)

      await flushRegisteredPreferences()
      expect(saved).toHaveLength(1)
    } finally {
      if (previousInternals) Object.defineProperty(window, '__TAURI_INTERNALS__', previousInternals)
      else delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
    }
  })

  it('preserves Post and Response drafts when a Thread is closed and reopened', async () => {
    await render()
    await act(async () => { select('Release review'); await settle() })
    fill('.composer-panel textarea', 'Post draft survives close')
    const reply = [...host.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent?.includes('Reply · No. 1'))
    expect(reply).toBeTruthy()
    await act(async () => { reply?.click() })
    fill('.composer-panel textarea', 'Response draft survives close')

    await act(async () => { host.querySelector<HTMLButtonElement>('button[aria-label="Close Thread"]')?.click(); await settle() })
    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    expect(host.querySelector('.composer-panel')).toBeNull()

    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toBe('Post draft survives close')
    const reopenedReply = [...host.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent?.includes('Reply · No. 1'))
    expect(reopenedReply).toBeTruthy()
    await act(async () => { reopenedReply?.click() })
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toBe('Response draft survives close')
  })

  it('discards delayed Thread data and permission results after closing during selection', async () => {
    let resolveThread!: (thread: Thread) => void
    let resolvePosts!: (page: ReturnType<typeof postPage>) => void
    let resolveCapabilities!: (result: ConversationCapabilities) => void
    vi.mocked(guiRead).mockImplementation((operation) => {
      if (operation === 'conversation.capabilities') return new Promise((resolve) => { resolveCapabilities = resolve }) as never
      if (operation === 'conversation.deleted') return Promise.resolve({ boards: [], threads: [] }) as never
      if (operation === 'board.list') return Promise.resolve({ items: [board] }) as never
      if (operation === 'thread.list') return Promise.resolve({ items: [threadA, threadPrivate] }) as never
      if (operation === 'thread.get') return new Promise((resolve) => { resolveThread = resolve }) as never
      if (operation === 'post.page') return new Promise((resolve) => { resolvePosts = resolve }) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review') })
    await act(async () => { host.querySelector<HTMLButtonElement>('button[aria-label="Close Thread"]')?.click() })

    await act(async () => {
      resolveThread(threadA)
      resolvePosts(postPage([post(1, 'Late response must stay closed')]))
      resolveCapabilities(editorCapabilities)
      await settle()
    })
    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')).toBeNull()
    expect(host.textContent).not.toContain('Late response must stay closed')
    expect(host.querySelector('.composer-pane')).toBeNull()
  })

  it('does not let a delayed Tree refresh reopen a Thread after it was closed', async () => {
    let refreshing = false
    let refreshDetailReads = 0
    let resolveThread!: (thread: Thread) => void
    let resolvePosts!: (page: ReturnType<typeof postPage>) => void
    vi.mocked(guiRead).mockImplementation((operation) => {
      if (operation === 'conversation.capabilities') return Promise.resolve(editorCapabilities) as never
      if (operation === 'conversation.deleted') return Promise.resolve({ boards: [], threads: [] }) as never
      if (operation === 'board.list') return Promise.resolve({ items: [board] }) as never
      if (operation === 'thread.list') return Promise.resolve({ items: [threadA, threadPrivate] }) as never
      if (operation === 'thread.get') {
        if (refreshing && ++refreshDetailReads === 1) return new Promise((resolve) => { resolveThread = resolve }) as never
        return Promise.resolve(threadA) as never
      }
      if (operation === 'post.page') {
        if (refreshing) return new Promise((resolve) => { resolvePosts = resolve }) as never
        return Promise.resolve(postPage([post(1, 'Refresh result')])) as never
      }
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    refreshing = true
    await act(async () => { button('Refresh All Threads').click(); await settle() })
    const close = host.querySelector<HTMLButtonElement>('button[aria-label="Close Thread"]')
    expect(close).not.toBeNull()
    await act(async () => { close?.click() })
    await act(async () => {
      resolveThread(threadA)
      resolvePosts(postPage([post(1, 'Stale refresh result')]))
      await settle()
    })

    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')).toBeNull()
    expect(host.querySelector('.composer-pane')).toBeNull()
    expect(host.textContent).not.toContain('Stale refresh result')
  })

  it('does not auto-select a new Thread when its delayed receipt arrives after closing the open Thread', async () => {
    const createdThread: Thread = { ...threadA, id: 'thread-created', name: 'Created thread' }
    let resolveCreate!: (result: Thread) => void
    let resolveAck!: (status: { pending: boolean; operation: null; expired: boolean }) => void
    vi.mocked(guiCreate).mockImplementation(() => new Promise((resolve) => { resolveCreate = resolve }) as never)
    vi.mocked(guiAckCreate).mockImplementation(() => new Promise((resolve) => { resolveAck = resolve }) as never)
    await render()
    await act(async () => { select('Release review'); await settle() })
    await act(async () => { button('Create Thread').click() })
    const boardSelect = host.querySelector<HTMLSelectElement>('.creation-dialog .creation-form select')!
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(boardSelect, board.id)
      boardSelect.dispatchEvent(new Event('input', { bubbles: true }))
      boardSelect.dispatchEvent(new Event('change', { bubbles: true }))
    })
    fill('.creation-dialog .creation-form input', createdThread.name)
    const form = host.querySelector<HTMLFormElement>('.creation-dialog .creation-form')!
    await act(async () => { form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    await act(async () => { resolveCreate(createdThread); await settle() })
    expect(guiAckCreate).toHaveBeenCalledTimes(1)

    await act(async () => { button('Close').click(); await settle() })
    await act(async () => { host.querySelector<HTMLButtonElement>('button[aria-label="Close Thread"]')?.click(); await settle() })
    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    await act(async () => { resolveAck({ pending: false, operation: null, expired: false }); await settle() })

    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    expect(host.querySelector('.thread-heading, .timeline, .composer-pane')).toBeNull()
    expect(host.querySelector('.composer-pane')).toBeNull()
  })

  it('does not restore the Thread detail when a delayed create acknowledgement arrives after close', async () => {
    let resolveCreate!: (result: Post) => void
    let resolveAck!: (status: { pending: boolean; operation: null; expired: boolean }) => void
    vi.mocked(guiCreate).mockImplementation(() => new Promise((resolve) => { resolveCreate = resolve }) as never)
    vi.mocked(guiAckCreate).mockImplementation(() => new Promise((resolve) => { resolveAck = resolve }) as never)
    await render()
    await act(async () => { select('Release review'); await settle() })
    fill('.composer-panel textarea', 'Create before closing')
    const composer = host.querySelector<HTMLFormElement>('.composer-panel')!
    await act(async () => { composer.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(guiCreate).toHaveBeenCalledTimes(1)

    await act(async () => { host.querySelector<HTMLButtonElement>('button[aria-label="Close Thread"]')?.click(); await settle() })
    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    await act(async () => { resolveCreate(post(3, 'Created while closing')); await settle() })
    expect(guiAckCreate).toHaveBeenCalledTimes(1)
    await act(async () => { resolveAck({ pending: false, operation: null, expired: false }); await settle() })

    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')).toBeNull()
    expect(host.querySelector('.composer-pane')).toBeNull()
  })

  it('builds the Board Tree View and keeps accessible Threads separate when parent is redacted', async () => {
    await render()
    expect(host.querySelector('.tree-board .tree-board-toggle')?.textContent).toBe('Engineering')
    expect(host.textContent).toContain('Release review')
    expect(host.textContent).toContain('Accessible Threads')
    expect(host.textContent).toContain('Private thread')
    expect(host.querySelector('[aria-label="Engineering"]')?.textContent).not.toContain('Private thread')
    const tree = host.querySelector<HTMLElement>('.thread-tree[aria-label="Boards"]')!
    const boardTreeItem = tree.querySelector('.tree-board')!
    expect(boardTreeItem.querySelector('.tree-threads .tree-thread')?.textContent).toBe('Release review')
    await act(async () => { select('Release review'); await settle() })
    const conversation = host.querySelector<HTMLElement>('.conversation-column')!
    expect(conversation.querySelector('.thread-detail[aria-label="Release review"]')).toBeTruthy()
    expect(conversation.querySelector('.composer-pane[aria-label="Post"]')).toBeTruthy()
    expect(conversation.querySelector('.thread-tree')).toBeNull()
    expect(host.querySelectorAll('.thread-tree').length).toBe(1)
    // jsdom checks semantic structure only; it does not verify rendered pane size or placement.
  })

  it('keeps the Tree View readable for a default member while hiding every creation control', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return memberCapabilities as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Readable Post')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    expect(host.textContent).toContain('Engineering')
    expect(host.querySelector('[aria-label="Create Board"]')).toBeNull()
    expect(host.querySelector('[aria-label="Create Thread"]')).toBeNull()
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()
    expect([...host.querySelectorAll('button')].some((item) => /Create Board|Create Thread/.test(item.textContent ?? ''))).toBe(false)
    await act(async () => { select('Release review'); await settle() })
    expect(host.textContent).toContain('Readable Post')
    expect(host.querySelector('.composer-panel')).toBeNull()
    expect(host.querySelector('.reply-action')).toBeNull()
    expect(host.querySelector('[data-testid="composer-height-separator"]')).toBeNull()
  })

  it('shows only permitted toolbar creation actions and limits the Thread dialog to individually granted Boards', async () => {
    const secondBoard = { ...board, id: 'board-b', name: 'Operations' }
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return { board_create: true, boards: [{ id: board.id, thread_create: false }, { id: secondBoard.id, thread_create: true }], threads: [] } as never
      if (operation === 'board.list') return { items: [board, secondBoard] } as never
      if (operation === 'thread.list') return { items: [] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    expect(host.querySelectorAll('[role="dialog"].creation-dialog')).toHaveLength(0)
    expect(button('Create Board')).toBeTruthy()
    expect(button('Create Thread')).toBeTruthy()
    await act(async () => { button('Create Thread').click() })
    const dialog = host.querySelector<HTMLElement>('[role="dialog"].creation-dialog[aria-modal="true"]')!
    expect(dialog.getAttribute('aria-label')).toBe('New Thread')
    const options = [...dialog.querySelector<HTMLSelectElement>('.creation-form select')!.options].map((option) => option.value)
    expect(options).toEqual(['', secondBoard.id])
  })

  it('shows Post and Response controls independently according to the selected Thread grants', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return { board_create: false, boards: [], threads: [{ id: threadA.id, post_create: true, response_create: false, post_edit: false, post_history: false }] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Post')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector('.composer-pane')?.getAttribute('aria-label')).toBe('Post')
    expect(host.querySelector('.reply-action')).toBeNull()

    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return { board_create: false, boards: [], threads: [{ id: threadA.id, post_create: false, response_create: true, post_edit: false, post_history: false }] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Post')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector('.composer-panel')).toBeNull()
    const reply = [...host.querySelectorAll('button')].find((item) => item.textContent?.includes('Reply · No. 1'))
    expect(reply).toBeTruthy()
    await act(async () => { reply?.click() })
    expect(host.querySelector('.composer-pane')?.getAttribute('aria-label')).toBe('Reply No. 1')
  })

  it('clears cached search excerpts when another Thread loses access', async () => {
    try {
      const previous = vi.mocked(guiRead).getMockImplementation()!
      let revoked = false
      vi.mocked(guiRead).mockImplementation(async (operation, args) => {
        if (operation === 'conversation.capabilities') return { ...editorCapabilities, threads: revoked ? editorCapabilities.threads.filter(item => item.id !== threadPrivate.id) : editorCapabilities.threads } as never
        if (operation === 'search') return { items: [searchItem('post', 'private-result', { thread_id: threadPrivate.id, excerpt: 'Revoked search secret' })], next_cursor: null, total_count: 1 } as never
        if (operation === 'post.list') return { items: [], next_before_no: null } as never
        return previous(operation, args)
      })
      await render()
      await act(async () => { select(threadA.name); await settle() })
      await act(async () => { testButton('workspace-search-open').click(); await settle() })
      fill('[data-testid="search-query"]', 'secret')
      await act(async () => { testButton('search-submit').click(); await settle() })
      expect(host.textContent).toContain('Revoked search secret')
      vi.useFakeTimers()
      revoked = true
      await act(async () => { window.dispatchEvent(new Event('focus')) })
      expect(host.textContent).not.toContain('Revoked search secret')
      expect(host.querySelector('[data-testid="search-dialog"]')).toBeNull()
    } finally { vi.useRealTimers() }
  })

  it('clears an already displayed private Thread when a background read check is denied', async () => {
    try {
      const previous = vi.mocked(guiRead).getMockImplementation()!
      let revoked = false
      vi.mocked(guiRead).mockImplementation(async (operation, args) => {
        if (operation === 'post.list' && args?.before_no === 1) {
          if (revoked) throw { exit_code: 4, code: 'PERMISSION_DENIED', message: 'denied' }
          return { items: [], next_before_no: null } as never
        }
        return previous(operation, args)
      })
      await render()
      await act(async () => { select('Release review'); await settle() })
      expect(host.textContent).toContain('Hello world')
      vi.useFakeTimers()
      revoked = true
      await act(async () => { window.dispatchEvent(new Event('focus')); await Promise.resolve(); await Promise.resolve() })
      expect(host.textContent).not.toContain('Hello world')
      expect(host.querySelector('[data-post-id]')).toBeNull()
      expect(host.querySelector('.composer-panel')).toBeNull()
    } finally { vi.useRealTimers() }
  })

  it('keeps conversation reading available when capability lookup fails', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') throw new Error('permission endpoint unavailable')
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Readable while grants fail')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    expect(host.textContent).toContain('Release review')
    expect(host.textContent).toContain('Could not check permissions.')
    await act(async () => { select('Release review'); await settle() })
    expect(host.textContent).toContain('Readable while grants fail')
    expect(host.querySelector('.composer-panel')).toBeNull()
    expect(host.querySelector('.reply-action')).toBeNull()
  })

  it('discards a late granted capability result after an access fault clears the private workspace', async () => {
    let capabilityCalls = 0
    let resolveLate!: (result: ConversationCapabilities) => void
    vi.mocked(guiRead).mockImplementation((operation) => {
      if (operation === 'conversation.capabilities') {
        capabilityCalls += 1
        if (capabilityCalls === 1) return Promise.resolve(editorCapabilities) as never
        return new Promise((resolve) => { resolveLate = resolve }) as never
      }
      if (operation === 'board.list') return Promise.resolve({ items: [board] }) as never
      if (operation === 'thread.list') return Promise.resolve({ items: [threadA] }) as never
      if (operation === 'thread.get') return Promise.resolve(threadA) as never
      if (operation === 'post.page') return Promise.reject({ code: 'PERMISSION_DENIED', exit_code: 4, message: 'Access denied' }) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await settle()
    await act(async () => { select('Release review'); await settle() })
    await act(async () => { resolveLate(editorCapabilities); await settle() })
    expect(host.textContent).toContain('Could not load the workspace')
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()
    expect(host.textContent).not.toContain('Create Board')
  })

  it('fails closed during delayed capability lookup and applies updated grants on refresh', async () => {
    let current = editorCapabilities
    let resolveDelayed!: (result: ConversationCapabilities) => void
    vi.mocked(guiRead).mockImplementation((operation) => {
      if (operation === 'conversation.capabilities') return new Promise((resolve) => { resolveDelayed = resolve }) as never
      if (operation === 'board.list') return Promise.resolve({ items: [board] }) as never
      if (operation === 'thread.list') return Promise.resolve({ items: [threadA] }) as never
      if (operation === 'thread.get') return Promise.resolve(threadA) as never
      if (operation === 'post.page') return Promise.resolve(postPage([post(1, 'Readable')])) as never
      return Promise.reject(new Error(`Unexpected read operation: ${operation}`)) as never
    })
    await render()
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()
    expect(host.textContent).toContain('Checking permissions…')
    await act(async () => { resolveDelayed(editorCapabilities); await settle() })
    await act(async () => { select('Release review'); await settle() })
    resolveDelayed(editorCapabilities)
    await act(async () => { await settle() })
    expect(host.querySelector('.composer-panel')).not.toBeNull()
    current = memberCapabilities
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return current as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Readable')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    const refresh = [...host.querySelectorAll('button')].find((item) => item.textContent === 'Refresh All Threads')
    await act(async () => { refresh?.click(); await settle() })
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()
    expect(host.querySelector('.composer-panel')).toBeNull()
    expect([...host.querySelectorAll('button')].some((item) => /Create Board|Create Thread/.test(item.textContent ?? ''))).toBe(false)
  })

  it('selects an authorized Thread Board when capabilities finish before the Tree View data', async () => {
    const deniedBoard = { ...board, id: 'board-denied', name: 'First Board' }
    const grantedBoard = { ...board, id: 'board-granted', name: 'Granted Board' }
    let resolveBoards!: (result: { items: Board[] }) => void
    vi.mocked(guiRead).mockImplementation((operation) => {
      if (operation === 'conversation.capabilities') return Promise.resolve({ board_create: false, boards: [{ id: deniedBoard.id, thread_create: false }, { id: grantedBoard.id, thread_create: true }], threads: [] }) as never
      if (operation === 'board.list') return new Promise((resolve) => { resolveBoards = resolve }) as never
      if (operation === 'thread.list') return Promise.resolve({ items: [] }) as never
      return Promise.reject(new Error(`Unexpected read operation: ${operation}`)) as never
    })
    await render()
    await act(async () => { resolveBoards({ items: [deniedBoard, grantedBoard] }); await settle() })
    expect(button('Create Thread')).toBeTruthy()
    expect([...host.querySelectorAll('button')].some((item) => item.textContent?.trim() === 'Create Board')).toBe(false)
    await act(async () => { button('Create Thread').click() })
    const selectBox = host.querySelector<HTMLSelectElement>('.creation-dialog .creation-form select')
    expect(selectBox?.value).toBe(grantedBoard.id)
    expect([...selectBox!.options].map((item) => item.value)).toEqual(['', grantedBoard.id])
  })

  it('keeps a Response draft separate when response permission is revoked', async () => {
    let grants = editorCapabilities
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return grants as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Post')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    const reply = [...host.querySelectorAll('button')].find((item) => item.textContent?.includes('Reply · No. 1'))
    await act(async () => { reply?.click() })
    fill('.composer-panel textarea', 'Reply stays a reply draft')
    grants = { ...editorCapabilities, threads: [{ id: threadA.id, post_create: true, response_create: false, post_edit: false, post_history: false, thread_delete: false, post_delete: false, post_restore: false }] }
    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector('.composer-pane')?.getAttribute('aria-label')).toBe('Post')
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toBe('')
    expect(guiCreate).not.toHaveBeenCalled()
  })

  it('shows the Timeline shape, redacts deleted bodies, and fetches direct Responses', async () => {
    await render()
    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector('.tree-thread.is-selected')?.getAttribute('aria-current')).toBe('page')
    expect(guiRead).toHaveBeenCalledWith('thread.get', { id: threadA.id })
    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 1, page_size: 50 })
    expect(host.textContent).toContain('No.1 ID: AB12 (Example Model: Codex)')
    expect(host.textContent).toContain('No.2 ID: CD34 (Unknown: Unknown)')
    expect(host.textContent).toContain('Hello world')
    expect(host.textContent).toContain('Deleted Post')
    expect(host.textContent).not.toContain('Deleted body')
    expect(host.querySelector('.post-body img, script')).toBeNull()
    const badge = host.querySelector<HTMLButtonElement>('.response-badge')
    expect(badge?.textContent).toContain('1 direct responses')
    await act(async () => { badge?.click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('response.list', { thread_id: threadA.id, to: 2, limit: 50 })
    expect(host.textContent).toContain('Direct reply')
  })

  it('drops the previous Thread immediately and ignores a late private-detail failure', async () => {
    await render()
    await act(async () => { select('Release review'); await settle() })
    expect(host.textContent).toContain('Hello world')
    const badge = host.querySelector<HTMLButtonElement>('.response-badge')
    await act(async () => { badge?.click(); await settle() })
    expect(host.textContent).toContain('Direct reply')
    let rejectPrivate!: (reason: unknown) => void
    vi.mocked(guiRead).mockImplementation((operation, args) => {
      if (operation === 'conversation.capabilities') return Promise.resolve(editorCapabilities) as never
      if (operation === 'thread.get' && args?.id === threadPrivate.id) return new Promise((_, reject) => { rejectPrivate = reject }) as never
      if (operation === 'post.page' && args?.thread_id === threadPrivate.id) return new Promise(() => {}) as never
      return Promise.reject(new Error('unexpected')) as never
    })
    await act(async () => { select('Private thread') })
    expect(host.textContent).not.toContain('Hello world')
    await act(async () => { rejectPrivate(new Error('permission revoked')); await settle() })
    expect(host.textContent).toContain('Could not load this Thread')
    expect(host.textContent).not.toContain('Review notes')
    expect(host.textContent).not.toContain('Direct reply')
  })

  it('ignores a delayed numbered-page result after the reader switches Threads', async () => {
    await render()
    let resolveNextPage!: (value: ReturnType<typeof postPage>) => void
    vi.mocked(guiRead).mockImplementation((operation, args) => {
      if (operation === 'thread.get') return (args?.id === threadPrivate.id ? threadPrivate : threadA) as never
      if (operation === 'post.page' && args?.thread_id === threadA.id && args.page === 2) {
        return new Promise((resolve) => { resolveNextPage = resolve }) as never
      }
      if (operation === 'post.page' && args?.thread_id === threadA.id) return Promise.resolve(postPage([post(50, 'First page')], 1, 50, 100)) as never
      if (operation === 'post.page' && args?.thread_id === threadPrivate.id) return new Promise(() => {}) as never
      return Promise.reject(new Error('unexpected')) as never
    })
    await act(async () => { select('Release review'); await settle() })
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-page-next-button"]')?.click() })
    await act(async () => { select('Private thread') })
    await act(async () => { resolveNextPage(postPage([post(51, 'Stale next page')], 2, 50, 100)); await settle() })
    expect(host.textContent).not.toContain('Stale next page')
    expect(host.textContent).not.toContain('First page')
    expect(host.querySelector('.tree-thread.is-selected')?.textContent).toBe('Private thread')
  })

  it('clears and discards delayed Responses when the reader changes numbered pages', async () => {
    let resolveResponses!: (page: { items: Post[]; next_before_no: number | null }) => void
    vi.mocked(guiRead).mockImplementation((operation, args) => {
      if (operation === 'conversation.capabilities') return Promise.resolve(editorCapabilities) as never
      if (operation === 'conversation.deleted') return Promise.resolve({ boards: [], threads: [] }) as never
      if (operation === 'board.list') return Promise.resolve({ items: [board] }) as never
      if (operation === 'thread.list') return Promise.resolve({ items: [threadA] }) as never
      if (operation === 'thread.get') return Promise.resolve(threadA) as never
      if (operation === 'post.page') {
        const page = Number(args?.page ?? 1)
        return Promise.resolve(postPage([post(page === 1 ? 1 : 51, `Visible page ${page}`, { response_count: page === 1 ? 1 : 0 })], page, 50, 100)) as never
      }
      if (operation === 'response.list') return new Promise((resolve) => { resolveResponses = resolve }) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    const responseBadge = host.querySelector<HTMLButtonElement>('.response-badge')!
    await act(async () => { responseBadge.click() })
    expect(host.textContent).toContain('Loading Thread')
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-page-number-2"]')?.click(); await settle() })
    expect(host.textContent).toContain('Visible page 2')
    expect(host.textContent).not.toContain('No. 1 · direct responses')
    await act(async () => { resolveResponses({ items: [post(2, 'Stale page-one response', { reply_to: 1 })], next_before_no: null }); await settle() })
    expect(host.textContent).not.toContain('Stale page-one response')
    expect(host.textContent).not.toContain('First response')
  })

  it('locks the Thread composer when a create races with the 1,000 Post cap', async () => {
    vi.mocked(guiCreate).mockRejectedValue({ code: 'THREAD_FULL', exit_code: 4, message: 'Thread has reached its 1,000 Post limit' })
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Near cap', { response_count: 1 })], 1, 50, 999) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    fill('.composer-panel textarea', 'One item too many')
    await act(async () => { host.querySelector<HTMLFormElement>('.composer-panel')?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(guiCreate).toHaveBeenCalledWith('post.create', { thread_id: threadA.id, body: 'One item too many' })
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel [data-testid="thread-post-limit-status"]')?.value).toContain('1,000 Post limit')
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toContain('One item too many')
    expect(host.querySelector('.composer-panel > .thread-post-limit')).toBeNull()
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.disabled).toBe(true)
  })

  it.each([
    [0, null], [98, '98 direct responses'], [99, '99+ direct responses'], [100, '99+ direct responses'],
  ])('shows the direct Response badge rule for count %i', async (count, label) => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Post', { response_count: count })]) as never
      throw new Error('unexpected')
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    if (label === null) expect(host.querySelector('.response-badge')).toBeNull()
    else expect(host.querySelector('.response-badge')?.textContent).toBe(label)
  })

  it('leaves the detail region out of loading state when refresh no longer returns the selected Thread', async () => {
    await render()
    await act(async () => { select('Release review'); await settle() })
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [] } as never
      throw new Error('unexpected')
    })
    const refresh = [...host.querySelectorAll('button')].find((button) => button.textContent === 'Refresh All Threads')
    await act(async () => { refresh?.click(); await settle() })
    expect(host.textContent).not.toContain('Loading Thread')
    expect(host.textContent).toContain('Select a Thread to read the conversation.')
  })

  it('clears the conversation and response preview when response access is revoked', async () => {
    await render()
    await act(async () => { select('Release review'); await settle() })
    expect(host.textContent).toContain('Hello world')
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'response.list') throw { code: 'PERMISSION_DENIED', exit_code: 4, message: 'Access denied' }
      throw new Error('unexpected')
    })
    const badge = host.querySelector<HTMLButtonElement>('.response-badge')
    await act(async () => { badge?.click(); await settle() })
    expect(host.textContent).not.toContain('Hello world')
    expect(host.textContent).not.toContain('Direct reply')
    expect(host.textContent).toContain('Could not load the workspace')
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()
  })

  it('does not let an old direct-response page keep a newer response list busy', async () => {
    await render()
    let resolveOlder!: (value: { items: Post[]; next_before_no: number | null }) => void
    vi.mocked(guiRead).mockImplementation((operation, args) => {
      if (operation === 'conversation.capabilities') return Promise.resolve(editorCapabilities) as never
      if (operation === 'board.list') return Promise.resolve({ items: [board] }) as never
      if (operation === 'thread.list') return Promise.resolve({ items: [threadA] }) as never
      if (operation === 'thread.get') return Promise.resolve(threadA) as never
      if (operation === 'post.page') return Promise.resolve(postPage([post(1, 'One', { response_count: 1 }), post(2, 'Two', { response_count: 1 })])) as never
      if (operation === 'response.list' && args?.to === 1 && args.before_no) return new Promise((resolve) => { resolveOlder = resolve }) as never
      if (operation === 'response.list' && args?.to === 1) return Promise.resolve({ items: [post(3, 'First response', { reply_to: 1 })], next_before_no: 3 }) as never
      if (operation === 'response.list' && args?.to === 2) return Promise.resolve({ items: [post(4, 'Second response', { reply_to: 2 })], next_before_no: null }) as never
      throw new Error('unexpected')
    })
    await act(async () => { select('Release review'); await settle() })
    const badges = host.querySelectorAll<HTMLButtonElement>('.response-badge')
    await act(async () => { badges[0].click(); await settle() })
    const older = [...host.querySelectorAll('button')].find((button) => button.textContent === 'Load older Posts')
    await act(async () => { older?.click() })
    await act(async () => { badges[1].click(); await settle() })
    expect(host.textContent).toContain('Second response')
    await act(async () => { resolveOlder({ items: [post(5, 'Stale response page', { reply_to: 1 })], next_before_no: null }); await settle() })
    expect(host.textContent).not.toContain('Stale response page')
    expect(host.textContent).toContain('Second response')
    expect(host.textContent).not.toContain('Loading older Posts')
  })
})

describe('Post editing and history', () => {
  async function openThread() {
    await render()
    await act(async () => { select('Release review'); await settle() })
  }

  it('shows edit and history actions only for live Posts with their separate grants', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return { ...editorCapabilities, threads: [{ id: threadA.id, post_create: false, response_create: false, post_edit: true, post_history: false }] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Live'), post(2, 'secret deleted body', { deleted: true })]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await openThread()
    expect(host.querySelector('[aria-label="Edit No. 1"]')).toBeTruthy()
    expect(host.querySelector('[aria-label="History No. 1"]')).toBeNull()
    expect(host.querySelector('[aria-label="Edit No. 2"]')).toBeNull()
    expect(host.querySelector('[aria-label="History No. 2"]')).toBeNull()
    expect(host.textContent).not.toContain('secret deleted body')
  })

  it('validates a nonblank UTF-8 body and sends the fixed Thread, No., and revision', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(3, 'Response body', { reply_to: 1 })]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    vi.mocked(guiEdit).mockImplementation(async (args) => post(args.no, args.body, { revision: args.if_revision + 1, reply_to: 1 }))
    await openThread()
    await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="Edit No. 3"]')?.click(); await settle() })
    fill('.post-edit-panel textarea', '   ')
    await act(async () => { button('Save changes').click(); await settle() })
    expect(host.textContent).toContain('Enter a message before posting.')
    expect(guiEdit).not.toHaveBeenCalled()

    fill('.post-edit-panel textarea', 'Edited Response')
    await act(async () => { button('Save changes').click(); await settle() })
    expect(guiEdit).toHaveBeenCalledWith({ thread_id: threadA.id, no: 3, body: 'Edited Response', if_revision: 1 })
  })

  it('keeps the draft after a revision conflict and resubmits only after explicit latest-version review', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Original')]) as never
      if (operation === 'post.get') return post(1, 'Other editor version', { revision: 2 }) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
  vi.mocked(guiEdit).mockImplementationOnce(async () => { throw { exit_code: 6 } })
  vi.mocked(guiEdit).mockImplementationOnce(async (args) => post(args.no, args.body, { revision: args.if_revision + 1 }))
    await openThread()
    await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="Edit No. 1"]')?.click(); await settle() })
    fill('.post-edit-panel textarea', 'My draft')
    await act(async () => { button('Save changes').click(); await settle() })
    expect(host.textContent).toContain('This Post changed after you opened it.')
    expect(host.querySelector<HTMLButtonElement>('.post-edit-panel button[type="submit"]')?.disabled).toBe(true)
    expect(host.querySelector<HTMLTextAreaElement>('.post-edit-panel textarea')?.value).toBe('My draft')
    await act(async () => { button('Reload latest version').click(); await settle() })
    expect(host.textContent).toContain('Other editor version')
    expect(host.querySelector<HTMLTextAreaElement>('.post-edit-panel textarea')?.value).toBe('My draft')
    await act(async () => { button('Save changes').click(); await settle() })
    expect(guiEdit).toHaveBeenLastCalledWith({ thread_id: threadA.id, no: 1, body: 'My draft', if_revision: 2 })
    expect(host.querySelector('.post-edit-panel')).toBeNull()
    expect(host.textContent).toContain('Changes saved.')
  })

  it.each([
    ['communication failure', { exit_code: 8 }],
    ['unknown invoke failure', new Error('transport closed')],
  ])('%s keeps editing blocked until the current Post is reloaded', async (_label, failure) => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Original')]) as never
      if (operation === 'post.get') return post(1, 'Current after uncertain request', { revision: 2 }) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    vi.mocked(guiEdit).mockRejectedValueOnce(failure)
    vi.mocked(guiEdit).mockResolvedValueOnce(post(1, 'My draft', { revision: 3 }))
    await openThread()
    await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="Edit No. 1"]')?.click(); await settle() })
    fill('.post-edit-panel textarea', 'My draft')
    await act(async () => { button('Save changes').click(); await settle() })
    expect(host.querySelector<HTMLButtonElement>('.post-edit-panel button[type="submit"]')?.disabled).toBe(true)
    expect(host.querySelector<HTMLTextAreaElement>('.post-edit-panel textarea')?.value).toBe('My draft')
    await act(async () => { button('Reload latest version').click(); await settle() })
    expect(host.textContent).toContain('Current after uncertain request')
    await act(async () => { button('Save changes').click(); await settle() })
    expect(guiEdit).toHaveBeenLastCalledWith({ thread_id: threadA.id, no: 1, body: 'My draft', if_revision: 2 })
  })

  it('keeps a correctable input rejection editable without replacing the draft', async () => {
    vi.mocked(guiEdit).mockRejectedValueOnce({ exit_code: 2 })
    vi.mocked(guiEdit).mockResolvedValueOnce(post(1, 'Corrected draft', { revision: 2 }))
    await openThread()
    await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="Edit No. 1"]')?.click(); await settle() })
    fill('.post-edit-panel textarea', 'First draft')
    await act(async () => { button('Save changes').click(); await settle() })
    expect(host.querySelector<HTMLTextAreaElement>('.post-edit-panel textarea')?.value).toBe('First draft')
    expect(host.querySelector<HTMLButtonElement>('.post-edit-panel button[type="submit"]')?.disabled).toBe(false)
    expect(host.querySelector('button')?.textContent).not.toContain('Reload latest version')
    fill('.post-edit-panel textarea', 'Corrected draft')
    await act(async () => { button('Save changes').click(); await settle() })
    expect(guiEdit).toHaveBeenLastCalledWith({ thread_id: threadA.id, no: 1, body: 'Corrected draft', if_revision: 1 })
    expect(host.querySelector('.post-edit-panel')).toBeNull()
  })

  it('serializes same-tick edit submissions and discards an edit result after Thread switching', async () => {
    let resolveEdit!: (item: Post) => void
    vi.mocked(guiEdit).mockReturnValueOnce(new Promise((resolve) => { resolveEdit = resolve }))
    await openThread()
    await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="Edit No. 1"]')?.click(); await settle() })
    fill('.post-edit-panel textarea', 'Changed body')
    const form = host.querySelector<HTMLFormElement>('.post-edit-panel')!
    await act(async () => {
      form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
      form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
      await settle()
    })
    expect(guiEdit).toHaveBeenCalledTimes(1)
    await act(async () => { select('Private thread'); await settle() })
    expect(host.querySelector('[aria-label="Edit No. 1"]')).toBeNull()
    resolveEdit(post(1, 'Changed body', { revision: 2 }))
    await act(async () => { await settle() })
    expect(host.querySelector('.post-edit-panel')).toBeNull()
    expect(host.textContent).not.toContain('Changes saved.')
    expect(host.querySelector('[aria-label="Edit No. 1"]')).toBeTruthy()
    await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="Edit No. 1"]')?.click(); await settle() })
    expect(host.querySelector('.post-edit-panel')).toBeTruthy()
  })

  it('blocks every numbered page control while a Post edit is saving, then re-enables navigation on success', async () => {
    let resolveEdit!: (item: Post) => void
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') {
        const page = Number(args?.page ?? 1)
        const pageSize = Number(args?.page_size ?? 50)
        return postPage([page === 1 ? post(1, 'Original') : post(51, 'Page two')], page, pageSize, 1000) as never
      }
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    vi.mocked(guiEdit).mockReturnValueOnce(new Promise((resolve) => { resolveEdit = resolve }))
    await openThread()
    await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="Edit No. 1"]')?.click(); await settle() })
    fill('.post-edit-panel textarea', 'Edited after paging unlocks')
    const form = host.querySelector<HTMLFormElement>('.post-edit-panel')!
    await act(async () => { form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })) })

    const pageControls = [
      'thread-page-first-button', 'thread-page-previous-button', 'thread-page-number-1',
      'thread-page-number-2', 'thread-page-next-button', 'thread-page-last-button',
    ]
    for (const testId of pageControls) expect(host.querySelector<HTMLButtonElement>(`[data-testid="${testId}"]`)?.disabled).toBe(true)
    const pageSize = host.querySelector<HTMLSelectElement>('[data-testid="thread-page-size-select"]')!
    expect(pageSize.disabled).toBe(true)
    await act(async () => {
      host.querySelector<HTMLButtonElement>('[data-testid="thread-page-number-2"]')?.click()
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(pageSize, '100')
      pageSize.dispatchEvent(new Event('change', { bubbles: true }))
      await settle()
    })
    expect(vi.mocked(guiRead).mock.calls.filter(([operation, args]) => operation === 'post.page' && args?.page === 2)).toHaveLength(0)
    expect(host.querySelector('.post-edit-panel')).not.toBeNull()

    await act(async () => { resolveEdit(post(1, 'Edited after paging unlocks', { revision: 2 })); await settle() })
    expect(host.querySelector('.post-edit-panel')).toBeNull()
    expect(host.querySelector<HTMLButtonElement>('[data-testid="thread-page-first-button"]')?.disabled).toBe(true)
    expect(host.querySelector<HTMLButtonElement>('[data-testid="thread-page-previous-button"]')?.disabled).toBe(true)
    for (const testId of ['thread-page-number-1', 'thread-page-number-2', 'thread-page-next-button', 'thread-page-last-button']) {
      expect(host.querySelector<HTMLButtonElement>(`[data-testid="${testId}"]`)?.disabled).toBe(false)
    }
    expect(pageSize.disabled).toBe(false)
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="thread-page-number-2"]')?.click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('post.page', { thread_id: threadA.id, page: 2, page_size: 50 })
    expect(host.textContent).toContain('Page two')
  })

  it('shows the current Post separately from safely rendered past versions and restores focus on close', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Original')]) as never
      if (operation === 'post.get') return post(1, '<img src="x" onerror="run()">current', { revision: 2 }) as never
      if (operation === 'post.history') return { items: [{ revision: 1, body: '<script>run()</script>past', saved_at: '2026-10-07T03:00:00Z', editor_id: 'editor', editor_display_id: 'CD34', editor_model_name: 'Other Model' }] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await openThread()
    const opener = host.querySelector<HTMLButtonElement>('[aria-label="History No. 1"]')!
    await act(async () => { opener.click(); await settle() })
    expect(host.querySelector('[role="dialog"][aria-modal="true"]')).toBeTruthy()
    expect(host.textContent).toContain('Current version · revision 2')
    expect(host.textContent).toContain('Before revision 1')
    expect(host.textContent).toContain('Updated byOther Model: CD34')
    expect(host.querySelector('.post-body img, script')).toBeNull()
    expect(host.textContent).not.toContain('<script>run()</script>past')
    await act(async () => { button('Close history').click(); await settle() })
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(document.activeElement).toBe(opener)
  })

  it('discards late history results after switching Threads and clears private content on an access fault', async () => {
    let resolveHistory!: (value: { items: { revision: number; body: string; saved_at: string; editor_id: string; editor_display_id: string; editor_model_name: string }[] }) => void
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return (args?.id === threadPrivate.id ? threadPrivate : threadA) as never
      if (operation === 'post.page') return postPage([post(1, 'Private old Thread body')]) as never
      if (operation === 'post.get') return post(1, 'Current body') as never
      if (operation === 'post.history') return new Promise((resolve) => { resolveHistory = resolve }) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await openThread()
    await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="History No. 1"]')?.click(); await settle() })
    await act(async () => { select('Private thread'); await settle() })
    resolveHistory({ items: [{ revision: 1, body: 'late secret', saved_at: '2026-10-07T03:00:00Z', editor_id: 'editor', editor_display_id: 'CD34', editor_model_name: 'Other Model' }] })
    await act(async () => { await settle() })
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(host.textContent).not.toContain('late secret')

    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Private old Thread body')]) as never
      if (operation === 'post.get') return post(1, 'Current body') as never
      if (operation === 'post.history') throw { exit_code: 3 }
      throw new Error(`Unexpected read operation: ${operation} ${String(args)}`)
    })
    await act(async () => { select('Release review'); await settle() })
    await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="History No. 1"]')?.click(); await settle() })
    expect(host.querySelector('.master-detail')).toBeNull()
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(host.textContent).not.toContain('Private old Thread body')
  })
})

describe('creation forms and Post Composer', () => {
  it('sends Board and Thread creation through their typed operations and selects a new Thread', async () => {
    vi.mocked(guiCreate).mockImplementation(async (operation) => {
      if (operation === 'board.create') return board
      return { ...threadA, id: 'thread-created', name: 'Created thread' }
    })
    await render()
    expect(host.querySelectorAll('[role="dialog"].creation-dialog')).toHaveLength(0)
    await act(async () => { button('Create Board').click() })
    expect(host.querySelector('[role="dialog"]')?.getAttribute('aria-label')).toBe('New Board')
    fill('.creation-dialog .creation-form input', 'Planning')
    await act(async () => { host.querySelector<HTMLButtonElement>('.creation-dialog footer button[type="submit"]')!.click(); await settle() })
    expect(guiCreate).toHaveBeenCalledWith('board.create', { name: 'Planning', description: '' })
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()

    await act(async () => { button('Create Thread').click() })
    expect(host.querySelector('[role="dialog"]')?.getAttribute('aria-label')).toBe('New Thread')
    const boardSelect = host.querySelector<HTMLSelectElement>('.creation-dialog .creation-form select')!
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(boardSelect, board.id)
      boardSelect.dispatchEvent(new Event('input', { bubbles: true }))
      boardSelect.dispatchEvent(new Event('change', { bubbles: true }))
    })
    fill('.creation-dialog .creation-form input', 'Created thread')
    await act(async () => { host.querySelector<HTMLButtonElement>('.creation-dialog footer button[type="submit"]')!.click(); await settle() })
    expect(guiCreate).toHaveBeenLastCalledWith('thread.create', { board_id: board.id, name: 'Created thread', description: '' })
    expect(guiRead).toHaveBeenCalledWith('thread.get', { id: 'thread-created' })
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()
  })

  it('keeps creation drafts on close, traps keyboard focus, and restores focus to each toolbar action', async () => {
    await render()
    const boardOpener = button('Create Board')
    boardOpener.focus()
    await act(async () => { boardOpener.click() })
    let dialog = host.querySelector<HTMLElement>('[role="dialog"].creation-dialog[aria-label]')!
    expect(dialog.getAttribute('aria-modal')).toBe('true')
    expect(dialog.getAttribute('aria-label')).toBe('New Board')
    expect(dialog.querySelector('.creation-dialog-heading, #creation-dialog-title')).toBeNull()
    expect(dialog.lastElementChild?.tagName).toBe('FOOTER')
    const footerSubmit = dialog.querySelector<HTMLButtonElement>('footer button[type="submit"]')!
    expect(footerSubmit.form).toBe(dialog.querySelector('.creation-form'))
    expect(host.querySelector('.workspace-background')?.hasAttribute('inert')).toBe(true)
    expect(host.querySelector('.workspace-background')?.getAttribute('aria-hidden')).toBe('true')
    expect(dialog.querySelector('input')?.value).toBe('')
    expect(dialog.contains(document.activeElement)).toBe(true)
    fill('.creation-dialog .creation-form input', 'Board draft')

    const focusable = [...dialog.querySelectorAll<HTMLElement>('input:not([disabled]), textarea:not([disabled]), select:not([disabled]), button:not([disabled])')]
    const firstFocusable = focusable[0]
    focusable[focusable.length - 1].focus()
    focusable[focusable.length - 1].dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }))
    expect(document.activeElement).toBe(firstFocusable)

    await act(async () => { dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })) })
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()
    expect(host.querySelector('.workspace-background')?.hasAttribute('inert')).toBe(false)
    expect(host.querySelector('.workspace-background')?.hasAttribute('aria-hidden')).toBe(false)
    expect(document.activeElement).toBe(boardOpener)
    await act(async () => { boardOpener.click() })
    dialog = host.querySelector<HTMLElement>('[role="dialog"].creation-dialog')!
    expect(dialog.querySelector<HTMLInputElement>('.creation-form input')?.value).toBe('Board draft')

    const threadOpener = button('Create Thread')
    await act(async () => { [...dialog.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent?.trim() === 'Close')?.click() })
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()
    expect(document.activeElement).toBe(boardOpener)
    threadOpener.focus()
    await act(async () => { threadOpener.click() })
    dialog = host.querySelector<HTMLElement>('[role="dialog"].creation-dialog')!
    expect(dialog.getAttribute('aria-label')).toBe('New Thread')
    expect(dialog.querySelector<HTMLButtonElement>('footer button[type="submit"]')?.form).toBe(dialog.querySelector('.creation-form'))
    fill('.creation-dialog .creation-form input', 'Thread draft')
    await act(async () => { [...dialog.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent?.trim() === 'Close')?.click() })
    expect(document.activeElement).toBe(threadOpener)
    threadOpener.focus()
    await act(async () => { threadOpener.click() })
    expect(host.querySelector<HTMLInputElement>('.creation-dialog .creation-form input')?.value).toBe('Thread draft')
  })

  it('keeps a failed Board create dialog and draft, and waits for durable acknowledgement before closing on success', async () => {
    await render()
    await act(async () => { button('Create Board').click() })
    fill('.creation-dialog .creation-form input', 'Retry this Board')
    vi.mocked(guiCreate).mockRejectedValueOnce(new Error('creation failed'))
    const form = host.querySelector<HTMLFormElement>('.creation-dialog .creation-form')!
    await act(async () => { form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(host.querySelector('[role="dialog"].creation-dialog')).not.toBeNull()
    expect(host.querySelector<HTMLInputElement>('.creation-dialog .creation-form input')?.value).toBe('Retry this Board')

    let acknowledge!: () => void
    vi.mocked(guiCreate).mockResolvedValueOnce(board as never)
    vi.mocked(guiAckCreate).mockReturnValueOnce(new Promise((resolve) => { acknowledge = () => resolve({ pending: false, operation: null, expired: false }) }) as never)
    await act(async () => { form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(host.querySelector('[role="dialog"].creation-dialog')).not.toBeNull()
    expect(host.querySelector<HTMLInputElement>('.creation-dialog .creation-form input')?.value).toBe('Retry this Board')
    await act(async () => { acknowledge(); await settle() })
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()
  })

  it('closes an open private creation dialog when a permission refresh revokes its grant', async () => {
    let grants = editorCapabilities
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return grants as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { button('Create Thread').click() })
    expect(host.querySelector('[role="dialog"].creation-dialog')).not.toBeNull()
    grants = memberCapabilities
    await act(async () => { button('Refresh All Threads').click(); await settle() })
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()
    expect([...host.querySelectorAll('button')].some((item) => /Create Board|Create Thread/.test(item.textContent ?? ''))).toBe(false)
  })

  it('closes an open creation dialog after a workspace permission read failure', async () => {
    await render()
    await act(async () => { button('Create Board').click() })
    expect(host.querySelector('[role="dialog"].creation-dialog')).not.toBeNull()
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') throw { code: 'PERMISSION_DENIED', exit_code: 4, message: 'Access denied' }
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await act(async () => { button('Refresh All Threads').click(); await settle() })
    expect(host.querySelector('[role="dialog"].creation-dialog')).toBeNull()
    expect(host.textContent).toContain('Could not load the workspace')
  })

  it('keeps a failed Post draft and enforces the UTF-8 byte limit before IPC', async () => {
    await render()
    await act(async () => { select('Release review'); await settle() })
    vi.mocked(guiCreate).mockRejectedValue({ code: 'PIPE_UNAVAILABLE', exit_code: 8, message: 'offline' })
    fill('.composer-panel textarea', 'Draft stays here')
    const composer = host.querySelector<HTMLFormElement>('.composer-panel')!
    await act(async () => { composer.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(guiCreate).toHaveBeenCalledWith('post.create', { thread_id: threadA.id, body: 'Draft stays here' })
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toBe('Draft stays here')
    expect(host.textContent).toContain('Your Post draft is still here')

    fill('.composer-panel textarea', '😀'.repeat(262145))
    await act(async () => { composer.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(host.textContent).toContain('The message exceeds the 1 MiB UTF-8 limit.')
    expect(guiCreate).toHaveBeenCalledTimes(1)
  })

  it('uses an explicit live-Post reply target and keeps drafts scoped to their Thread', async () => {
    await render()
    await act(async () => { select('Release review'); await settle() })
    const reply = [...host.querySelectorAll('button')].find((item) => item.textContent?.includes('Reply · No. 1'))
    expect(reply).toBeTruthy()
    expect([...host.querySelectorAll('button')].some((item) => item.textContent?.includes('Reply · No. 2'))).toBe(false)
    await act(async () => { reply?.click() })
    expect(host.textContent).toContain('Reply to No. 1')
    fill('.composer-panel textarea', 'Thread A response draft')
    await act(async () => { select('Private thread'); await settle() })
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toBe('')
    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toBe('Thread A response draft')
    vi.mocked(guiCreate).mockResolvedValue(post(3, 'Created response', { thread_id: threadA.id, reply_to: 1 }) as never)
    const composer = host.querySelector<HTMLFormElement>('.composer-panel')!
    await act(async () => { composer.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(guiCreate).toHaveBeenCalledWith('response.create', { thread_id: threadA.id, to: 1, body: 'Thread A response draft' })
  })

  it('blocks creation and logout during a pending request, and retries only on a human click', async () => {
    vi.mocked(guiCreationStatus).mockResolvedValueOnce({ pending: true, operation: 'post.create', expired: false })
      .mockResolvedValueOnce({ pending: false, operation: null, expired: false })
    vi.mocked(guiRetryCreate).mockResolvedValue(post(3, 'Recovered Post') as never)
    await render()
    expect(host.textContent).toContain('It may already have completed; discard removes only the retry record.')
    expect(button('Create Board').hasAttribute('disabled')).toBe(true)
    expect(button('Create Thread').hasAttribute('disabled')).toBe(true)
    expect(button('Log out').hasAttribute('disabled')).toBe(true)
    expect(guiRetryCreate).not.toHaveBeenCalled()
    await act(async () => { button('Retry saved creation').click(); await settle() })
    expect(guiRetryCreate).toHaveBeenCalledTimes(1)
    expect(guiCreate).not.toHaveBeenCalled()
  })

  it('does not apply a durable pending retry quota fault to a different selected Thread', async () => {
    vi.mocked(guiCreationStatus).mockResolvedValue({ pending: true, operation: 'post.create', expired: false })
    vi.mocked(guiRetryCreate).mockRejectedValue({ code: 'THREAD_FULL', exit_code: 4, message: 'original Thread is full' })
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get') return (args?.id === threadPrivate.id ? threadPrivate : threadA) as never
      if (operation === 'post.page') return postPage([post(1, 'Other Thread remains under quota', { thread_id: threadPrivate.id })], 1, 50, 12) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Private thread'); await settle() })
    expect(host.querySelector('.thread-detail')?.getAttribute('aria-label')).toBe('Private thread')
    await act(async () => { button('Retry saved creation').click(); await settle() })

    expect(guiRetryCreate).toHaveBeenCalledTimes(1)
    expect(host.querySelector('.thread-detail')?.getAttribute('aria-label')).toBe('Private thread')
    expect(host.querySelector('[data-testid="thread-post-limit-status"]')).toBeNull()
    expect(host.querySelectorAll('.timeline-post')).toHaveLength(1)
    expect(host.textContent).toContain('Other Thread remains under quota')
    expect(host.querySelector('[role="alert"]')?.textContent?.trim().length).toBeGreaterThan(0)
  })

  it('keeps the draft and retry record when receipt acknowledgement fails after creation succeeds', async () => {
    vi.mocked(guiCreationStatus).mockResolvedValueOnce({ pending: false, operation: null, expired: false })
      .mockResolvedValueOnce({ pending: true, operation: 'post.create', expired: false })
      .mockResolvedValueOnce({ pending: false, operation: null, expired: false })
    vi.mocked(guiCreate).mockResolvedValue(post(3, 'Created Post') as never)
    vi.mocked(guiAckCreate).mockRejectedValueOnce(new Error('ack transport failed'))
    vi.mocked(guiRetryCreate).mockResolvedValue(post(3, 'Created Post') as never)
    await render()
    await act(async () => { select('Release review'); await settle() })
    fill('.composer-panel textarea', 'Durable draft')
    const composer = host.querySelector<HTMLFormElement>('.composer-panel')!
    await act(async () => { composer.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(host.textContent).toContain('The item was created.')
    expect(host.textContent).toContain('Confirmation is still pending')
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toBe('Durable draft')
    expect(button('Retry saved creation').hasAttribute('disabled')).toBe(false)
    await act(async () => { button('Retry saved creation').click(); await settle() })
    expect(guiCreate).toHaveBeenCalledTimes(1)
    expect(guiRetryCreate).toHaveBeenCalledTimes(1)
    expect(guiAckCreate).toHaveBeenCalledTimes(2)
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toBe('')
  })

  it('treats an acknowledgement error as success when durable status confirms there is no pending receipt', async () => {
    vi.mocked(guiCreationStatus).mockResolvedValueOnce({ pending: false, operation: null, expired: false })
      .mockResolvedValueOnce({ pending: false, operation: null, expired: false })
      .mockResolvedValueOnce({ pending: false, operation: null, expired: false })
    vi.mocked(guiCreate).mockResolvedValue(post(3, 'Created Post') as never)
    vi.mocked(guiAckCreate).mockRejectedValueOnce(new Error('ack response was lost'))
    await render()
    await act(async () => { select('Release review'); await settle() })
    fill('.composer-panel textarea', 'No duplicate draft')
    const composer = host.querySelector<HTMLFormElement>('.composer-panel')!
    await act(async () => { composer.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() })
    expect(host.textContent).toContain('Created successfully.')
    expect(host.textContent).not.toContain('Confirmation is still pending')
    expect(host.querySelector<HTMLTextAreaElement>('.composer-panel textarea')?.value).toBe('')
    expect(guiCreate).toHaveBeenCalledTimes(1)
  })

  it('marks expired pending creations as non-retriable and allows explicit discard', async () => {
    vi.mocked(guiCreationStatus).mockResolvedValueOnce({ pending: true, operation: 'post.create', expired: true })
    await render()
    expect(host.textContent).toContain('cannot be resent automatically')
    expect(button('Retry saved creation').hasAttribute('disabled')).toBe(true)
    expect(button('Discard retry record').hasAttribute('disabled')).toBe(false)
    await act(async () => { button('Discard retry record').click(); await settle() })
    expect(guiDiscardCreate).toHaveBeenCalledTimes(1)
  })
})

describe('delete and restore management', () => {
  it.each(['single', 'bulk'] as const)('keeps the Workspace while its own %s purge removes a deleted row', async kind => {
    const previous = vi.mocked(guiRead).getMockImplementation()!
    let purged = false
    let finish!: () => void
    const pending = new Promise<void>(resolve => { finish = resolve })
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.deleted') return {
        boards: purged ? [] : [{ ...board, id: 'purge-only', name: 'Own purge Board', deleted_at: '2026-10-07T00:00:00Z', can_restore: true, can_purge: true }], threads: [],
      } as never
      return previous(operation, args)
    })
    vi.mocked(guiManage).mockImplementation(async () => { purged = true; await pending; return {} as never })
    vi.mocked(guiManageConversationBatch).mockImplementation(async () => { purged = true; await pending; return { action: 'purge', items: [] } })
    await render()
    await act(async () => { button('Deleted items').click(); await settle() })
    if (kind === 'bulk') {
      await act(async () => { button('Select').click() })
      await act(async () => { button('Select all').click() })
      await act(async () => { button('Permanently delete selected').click() })
    } else {
      await act(async () => { button('Permanently delete').click() })
    }
    await act(async () => { host.querySelector<HTMLButtonElement>('.management-confirm-dialog .button-danger')!.click(); await settle() })
    try {
      await act(async () => { window.dispatchEvent(new Event('focus')); await settle() })
      expect(host.textContent).toContain('Engineering')
      expect(host.querySelector('.management-confirm-dialog')).not.toBeNull()
    } finally {
      await act(async () => { finish(); await settle() })
    }
    expect(host.textContent).toContain('Engineering')
    expect(host.textContent).toContain('There are no restorable Boards or Threads.')
    expect(host.textContent).not.toContain('Refresh the workspace before another management action.')
  })

  it.each(['board', 'thread', 'purge', 'bulk'] as const)('clears cached deleted metadata and review when deleted-only %s rights are lost', async kind => {
    const previous = vi.mocked(guiRead).getMockImplementation()!
    let revoked = false
    const deletedBoard = { ...board, name: 'Revoked deleted Board', deleted_at: '2026-10-07T00:00:00Z', can_restore: true, can_purge: true }
    const deletedThread = { ...threadPrivate, name: 'Revoked deleted Thread', deleted_at: '2026-10-07T00:00:00Z', can_restore: true, can_purge: true }
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return { board_create: false, boards: [], threads: [] } as never
      if (operation === 'board.list' || operation === 'thread.list') return { items: [] } as never
      if (operation === 'conversation.deleted') return {
        boards: revoked && kind === 'board' ? [] : [deletedBoard],
        threads: revoked && kind === 'thread' ? [] : [{ ...deletedThread, can_purge: !(revoked && (kind === 'purge' || kind === 'bulk')) }],
      } as never
      return previous(operation, args)
    })
    await render()
    await act(async () => { button('Deleted items').click(); await settle() })
    expect(host.textContent).toContain('Revoked deleted Board')
    expect(host.textContent).toContain('Revoked deleted Thread')
    if (kind === 'bulk') {
      await act(async () => { button('Select').click() })
      await act(async () => { button('Select all').click() })
      await act(async () => { button('Permanently delete selected').click() })
      expect(host.querySelector('.management-confirm-dialog')).not.toBeNull()
    } else if (kind === 'purge') {
      await act(async () => {
        const rows = [...host.querySelectorAll('.deleted-items-list > li')]
        const row = rows.find(item => item.textContent?.includes('Revoked deleted Thread'))!
        const purge = [...row.querySelectorAll('button')].find(item => item.textContent === 'Permanently delete')!
        purge.click(); await settle()
      })
      expect(host.querySelector('.management-confirm-dialog')).not.toBeNull()
    }
    try {
      vi.useFakeTimers()
      revoked = true
      await act(async () => { window.dispatchEvent(new Event('focus')) })
      expect(host.textContent).not.toContain('Revoked deleted Board')
      expect(host.textContent).not.toContain('Revoked deleted Thread')
      expect(host.querySelector('.management-drawer')).toBeNull()
      expect(host.querySelector('.management-confirm-dialog')).toBeNull()
      expect(guiManage).not.toHaveBeenCalled()
      expect(guiManageConversationBatch).not.toHaveBeenCalled()
    } finally { vi.useRealTimers() }
  })

  it('hides Board, Thread, Post delete and Post restore controls when capabilities deny them', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return memberCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [{ id: board.id, name: board.name, description: '', created_at: board.created_at, deleted_at: '2026-10-07T00:00:00Z' }], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(2, 'secret tombstone', { deleted: true, deleted_at: '2026-10-07T03:00:00Z' })]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    expect(host.querySelector('.tree-manage-action')).toBeNull()
    expect(button('Deleted items').hasAttribute('disabled')).toBe(false)
    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector('.post-tool-action')).toBeNull()
    expect(host.textContent).not.toContain('secret tombstone')
    await act(async () => { button('Deleted items').click(); await settle() })
    expect(host.querySelector('.management-drawer')).not.toBeNull()
    expect(host.querySelector('.deleted-items-list .button')?.textContent).toBe('Restore')
    expect(host.textContent).not.toContain('secret tombstone')
  })

  it('shows impact in the confirmation Dialog and leaves the item untouched when canceled', async () => {
    await render()
    const boardDelete = host.querySelector<HTMLButtonElement>('.tree-manage-action')!
    await act(async () => { boardDelete.click() })
    expect(host.querySelector('[role="dialog"]')?.textContent).toContain('This Board and its Threads and Posts will be hidden')
    expect(host.querySelector('[role="dialog"]')?.textContent).toContain('Engineering')
    await act(async () => { button('Cancel').click() })
    expect(host.querySelector('.management-confirm-dialog')).toBeNull()
    expect(guiManage).not.toHaveBeenCalled()
  })

  it('describes whole-Board deletion without counting only permission-visible Threads', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return { ...editorCapabilities, threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    const boardDelete = host.querySelector<HTMLButtonElement>('.tree-manage-action')!
    await act(async () => { boardDelete.click() })
    const dialog = host.querySelector('.management-confirm-dialog')!
    expect(dialog.textContent).toContain('Engineering')
    expect(dialog.textContent).toContain('This Board and its Threads and Posts will be hidden')
    expect(dialog.textContent).not.toMatch(/\b\d+ Threads?\b/)
    expect(dialog.textContent).not.toContain('hidden Thread')
  })

  it('sends typed Board, Post delete and Post restore arguments after explicit confirmation', async () => {
    await render()
    await act(async () => { host.querySelector<HTMLButtonElement>('.tree-manage-action')!.click() })
    await act(async () => { button('Confirm').click(); await settle() })
    expect(guiManage).toHaveBeenNthCalledWith(1, 'board.delete', { id: board.id })
    await act(async () => { select('Release review'); await settle() })
    const deletePost = [...host.querySelectorAll<HTMLButtonElement>('.post-tool-action')].find((item) => item.getAttribute('aria-label') === 'Delete No. 1')
    expect(deletePost).toBeDefined()
    await act(async () => { deletePost!.click() })
    expect(host.querySelector('.management-confirm-dialog')?.textContent).toContain('Post body will be hidden')
    await act(async () => { button('Confirm').click(); await settle() })
    expect(guiManage).toHaveBeenNthCalledWith(2, 'post.delete', { thread_id: threadA.id, no: 1 })
    expect(host.textContent).not.toContain('Deleted body')
    const restorePost = [...host.querySelectorAll<HTMLButtonElement>('.post-tool-action')].find((item) => item.getAttribute('aria-label') === 'Restore No. 2')
    expect(restorePost).toBeDefined()
    await act(async () => { restorePost!.click() })
    expect(host.querySelector('.management-confirm-dialog')?.textContent).not.toContain('Deleted body')
    expect(guiRead).not.toHaveBeenCalledWith('post.get', { thread_id: threadA.id, no: 2 })
    await act(async () => { button('Confirm').click(); await settle() })
    expect(guiManage).toHaveBeenNthCalledWith(3, 'post.restore', { thread_id: threadA.id, no: 2 })
  })

  it('restores only listed Board and Thread rows without relying on live delete grants, and deletes a Thread by id', async () => {
    const hiddenParentThread: Thread = { id: 'deleted-child', board_id: 'redacted-board', name: 'Restorable Thread', description: '', created_at: threadA.created_at }
    const restrictedCaps: ConversationCapabilities = {
      board_create: false, boards: [],
      threads: [{ id: threadPrivate.id, post_create: false, response_create: false, post_edit: false, post_history: false, thread_delete: true, post_delete: false, post_restore: false }],
    }
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return restrictedCaps as never
      if (operation === 'conversation.deleted') return {
        boards: [{ id: 'deleted-board', name: 'Old Board', description: '', created_at: board.created_at, deleted_at: '2026-10-07T00:00:00Z' }],
        threads: [{ ...hiddenParentThread, deleted_at: '2026-10-07T00:00:00Z' }],
      } as never
      if (operation === 'board.list') return { items: [] } as never
      if (operation === 'thread.list') return { items: [threadPrivate] } as never
      if (operation === 'thread.get') return threadPrivate as never
      if (operation === 'post.page') return postPage([]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { button('Deleted items').click(); await settle() })
    expect(host.textContent).toContain('Old Board')
    expect(host.textContent).toContain('Restorable Thread')
    expect(host.textContent).not.toContain('redacted-board')
    expect(host.textContent).not.toContain('hidden-parent')
    const restoreBoard = [...host.querySelectorAll<HTMLButtonElement>('.deleted-items-list button')].find((item) => item.closest('li')?.textContent?.includes('Old Board'))!
    await act(async () => { restoreBoard.click() })
    expect(host.querySelector('.management-confirm-dialog')?.textContent).toContain('Old Board')
    await act(async () => { button('Confirm').click(); await settle() })
    expect(guiManage).toHaveBeenNthCalledWith(1, 'board.restore', { id: 'deleted-board' })
    const restoreThread = [...host.querySelectorAll<HTMLButtonElement>('.deleted-items-list button')].find((item) => item.closest('li')?.textContent?.includes('Restorable Thread'))!
    await act(async () => { restoreThread.click() })
    expect(host.querySelector('.management-confirm-dialog')?.textContent).not.toContain('redacted-board')
    await act(async () => { button('Confirm').click(); await settle() })
    expect(guiManage).toHaveBeenNthCalledWith(2, 'thread.restore', { id: hiddenParentThread.id })
    const threadDelete = [...host.querySelectorAll<HTMLButtonElement>('.tree-manage-action')].find((item) => item.textContent === 'Delete')!
    await act(async () => { threadDelete.click() })
    await act(async () => { button('Confirm').click(); await settle() })
    expect(guiManage).toHaveBeenNthCalledWith(3, 'thread.delete', { id: threadPrivate.id })
  })

  it('reviews and cancels individual permanent deletion, then sends the confirmed purge operation', async () => {
    const oldThread: Thread = { ...threadA, id: 'old-thread', name: 'Old Thread' }
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [{ id: 'old-board', name: 'Old Board', description: '', created_at: board.created_at, deleted_at: '2026-10-07T00:00:00Z', can_restore: true, can_purge: true }], threads: [{ ...oldThread, deleted_at: '2026-10-07T00:00:00Z', can_restore: true, can_purge: true }] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { button('Deleted items').click(); await settle() })
    const purge = [...host.querySelectorAll<HTMLButtonElement>('.deleted-item-actions button')].find((item) => item.textContent === 'Permanently delete')!
    await act(async () => { purge.click() })
    expect(host.querySelector('.management-confirm-dialog')?.textContent).toContain('all of its descendant Threads, Posts, revisions, and conversation state')
    await act(async () => { button('Cancel').click() })
    expect(guiManage).not.toHaveBeenCalled()
    await act(async () => { purge.click() })
    await act(async () => { host.querySelector<HTMLButtonElement>('.management-confirm-dialog .button-danger')!.click(); await settle() })
    expect(guiManage).toHaveBeenCalledWith('board.purge', { id: 'old-board', confirmed: true })
    const threadRow = [...host.querySelectorAll<HTMLElement>('.deleted-items-list li')].find((row) => row.textContent?.includes('Old Thread'))!
    await act(async () => { [...threadRow.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent === 'Permanently delete')!.click() })
    expect(host.querySelector('.management-confirm-dialog')?.textContent).toContain('This Thread, its Posts, revisions, and conversation state will be permanently removed')
    await act(async () => { host.querySelector<HTMLButtonElement>('.management-confirm-dialog .button-danger')!.click(); await settle() })
    expect(guiManage).toHaveBeenLastCalledWith('thread.purge', { id: 'old-thread', confirmed: true })
  })

  it('selects all displayed deleted items and lists the exact targets before bulk purge', async () => {
    const oldThread: Thread = { ...threadA, id: 'old-thread', name: 'Old Thread' }
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return {
        boards: [{ id: 'old-board', name: 'Old Board', description: '', created_at: board.created_at, deleted_at: '2026-10-07T00:00:00Z', can_restore: true, can_purge: true }],
        threads: [{ ...oldThread, deleted_at: '2026-10-07T00:00:00Z', can_restore: true, can_purge: true }],
      } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { button('Deleted items').click(); await settle() })
    await act(async () => { button('Select').click() })
    await act(async () => { button('Select all').click() })
    expect(host.textContent).toContain('2 selected')
    await act(async () => { button('Clear selection').click() })
    expect(host.textContent).toContain('0 selected')
    await act(async () => { button('Select all').click() })
    await act(async () => { button('Permanently delete selected').click() })
    const dialog = host.querySelector('.management-confirm-dialog')!
    expect(dialog.textContent).toContain('Old Board')
    expect(dialog.textContent).toContain('Old Thread')
    expect(dialog.textContent).toContain('Only the listed targets will be purged')
    await act(async () => { button('Cancel').click() })
    expect(guiManageConversationBatch).not.toHaveBeenCalled()
    await act(async () => { button('Permanently delete selected').click() })
    await act(async () => { host.querySelector<HTMLButtonElement>('.management-confirm-dialog .button-danger')!.click(); await settle() })
    expect(guiManageConversationBatch).toHaveBeenCalledWith({ action: 'purge', items: [{ kind: 'board', id: 'old-board' }, { kind: 'thread', id: 'old-thread' }], confirmed: true })
  })

  it('blocks individual child restore but allows an atomic restore when the deleted parent is selected too', async () => {
    const child: Thread = { ...threadA, id: 'deleted-child', board_id: 'old-board', name: 'Deleted Child' }
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return {
        boards: [{ id: 'old-board', name: 'Old Board', description: '', created_at: board.created_at, deleted_at: '2026-10-07T00:00:00Z', can_restore: true, can_purge: true }],
        threads: [{ ...child, deleted_at: '2026-10-07T00:00:00Z', parent_deleted: true, can_restore: true, can_purge: true }],
      } as never
      if (operation === 'board.list') return { items: [] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { button('Deleted items').click(); await settle() })
    const childRow = [...host.querySelectorAll<HTMLElement>('.deleted-items-list li')].find((row) => row.textContent?.includes('Deleted Child'))!
    expect([...childRow.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent === 'Restore')?.disabled).toBe(true)
    await act(async () => { button('Select').click() })
    const checkboxes = [...host.querySelectorAll<HTMLInputElement>('.deleted-item-select input')]
    await act(async () => { checkboxes[1].click() })
    expect(button('Restore selected').disabled).toBe(true)
    await act(async () => { checkboxes[0].click() })
    expect(button('Restore selected').disabled).toBe(false)
    await act(async () => { button('Restore selected').click() })
    const dialog = host.querySelector('.management-confirm-dialog')!
    expect(dialog.textContent).toContain('Old Board')
    expect(dialog.textContent).toContain('Deleted Child')
    await act(async () => { host.querySelector<HTMLButtonElement>('.management-confirm-dialog .button-primary')!.click(); await settle() })
    expect(guiManageConversationBatch).toHaveBeenCalledWith({ action: 'restore', items: [{ kind: 'board', id: 'old-board' }, { kind: 'thread', id: 'deleted-child' }], confirmed: true })
  })

  it('preserves selection and reports a safe review error when a bulk request is stale', async () => {
    let becameStale = false
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: becameStale ? [] : [{ id: 'old-board', name: 'Old Board', description: '', created_at: board.created_at, deleted_at: '2026-10-07T00:00:00Z', can_restore: true, can_purge: true }], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    vi.mocked(guiManageConversationBatch).mockImplementation(async () => { becameStale = true; throw new Error('stale target') })
    await render()
    await act(async () => { button('Deleted items').click(); await settle() })
    await act(async () => { button('Select').click() })
    await act(async () => { host.querySelector<HTMLInputElement>('.deleted-item-select input')!.click() })
    await act(async () => { button('Restore selected').click() })
    await act(async () => { host.querySelector<HTMLButtonElement>('.management-confirm-dialog .button-primary')!.click(); await settle() })
    expect(host.textContent).toContain('1 selected')
    expect(host.textContent).toContain('Some selected items are no longer in the refreshed list')
    expect(host.querySelectorAll('.deleted-items-list').length).toBe(0)
    expect(guiManageConversationBatch).toHaveBeenCalledTimes(1)
  })

  it('keeps only the top confirmation modal accessible and restores focus through Escape', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [{ id: 'deleted-board', name: 'Old Board', description: '', created_at: board.created_at, deleted_at: '2026-10-07T00:00:00Z' }], threads: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { button('Deleted items').click(); await settle() })
    const restore = host.querySelector<HTMLButtonElement>('.deleted-items-list button')!
    await act(async () => { restore.click() })
    const drawer = host.querySelector<HTMLElement>('.management-drawer')!
    const confirmation = host.querySelector<HTMLElement>('.management-confirm-dialog')!
    expect(drawer.getAttribute('aria-hidden')).toBe('true')
    expect(drawer.getAttribute('aria-modal')).toBe('false')
    expect(drawer.hasAttribute('inert')).toBe(true)
    expect(drawer.hasAttribute('role')).toBe(false)
    expect(host.querySelectorAll('[role="dialog"]').length).toBe(1)
    expect(confirmation.getAttribute('aria-modal')).toBe('true')
    expect(document.activeElement).toBe(button('Confirm'))
    await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true })) })
    expect(document.activeElement).toBe(button('Cancel'))
    await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true })) })
    expect(document.activeElement).toBe(button('Confirm'))
    await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await settle() })
    expect(host.querySelector('.management-confirm-dialog')).toBeNull()
    expect(drawer.getAttribute('aria-hidden')).toBe(null)
    expect(drawer.getAttribute('role')).toBe('dialog')
    expect(document.activeElement).toBe(button('Close deleted items'))
    await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await settle() })
    expect(host.querySelector('.management-drawer')).toBeNull()
    expect(document.activeElement).toBe(button('Deleted items'))
  })

  it('clears a deleted-items listing after a failed read and exposes no stale restore buttons', async () => {
    let fail = false
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') {
        if (fail) throw new Error('deleted listing unavailable')
        return { boards: [{ id: board.id, name: board.name, description: '', created_at: board.created_at, deleted_at: '2026-10-07T00:00:00Z' }], threads: [] } as never
      }
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { button('Deleted items').click(); await settle() })
    expect(host.textContent).toContain('Engineering')
    expect(host.querySelector('.deleted-items-list button')).not.toBeNull()
    await act(async () => { button('Close deleted items').click() })
    fail = true
    await act(async () => { button('Deleted items').click(); await settle() })
    expect(host.textContent).toContain('Could not load deleted items.')
    expect(host.querySelector('.deleted-items-list button')).toBeNull()
    expect(host.querySelector('.deleted-items-list')).toBeNull()
  })

  it('does not retry an uncertain management request and refreshes before enabling another action', async () => {
    vi.mocked(guiManage).mockRejectedValueOnce(new Error('response lost'))
    await render()
    await act(async () => { host.querySelector<HTMLButtonElement>('.tree-manage-action')!.click() })
    await act(async () => { button('Confirm').click(); await settle() })
    expect(guiManage).toHaveBeenCalledTimes(1)
    expect(host.textContent).toContain('The result could not be confirmed')
    expect(host.querySelector('.tree-manage-action')?.hasAttribute('disabled')).toBe(false)
    expect(guiManage).toHaveBeenCalledTimes(1)
  })

  it('ignores a delayed result after Thread navigation and requires a fresh explicit Tree View refresh', async () => {
    let resolveManage!: (value: Post) => void
    vi.mocked(guiManage).mockImplementation(() => new Promise((resolve) => { resolveManage = resolve }))
    await render()
    await act(async () => { host.querySelector<HTMLButtonElement>('.tree-manage-action')!.click() })
    await act(async () => { button('Confirm').click() })
    await act(async () => { select('Private thread'); await settle() })
    const capabilityReadsBefore = vi.mocked(guiRead).mock.calls.filter(([operation]) => operation === 'conversation.capabilities').length
    await act(async () => { resolveManage({} as Post); await settle() })
    const capabilityReadsAfter = vi.mocked(guiRead).mock.calls.filter(([operation]) => operation === 'conversation.capabilities').length
    expect(capabilityReadsAfter).toBe(capabilityReadsBefore)
    expect(host.textContent).toContain('Refresh the workspace before another management action')
    expect(host.querySelector<HTMLButtonElement>('.deleted-items-action')?.hasAttribute('disabled')).toBe(true)
    await act(async () => { button('Refresh All Threads').click(); await settle() })
    expect(host.textContent).not.toContain('Refresh the workspace before another management action')
    expect(host.querySelector<HTMLButtonElement>('.deleted-items-action')?.hasAttribute('disabled')).toBe(false)
  })

  it('expands a Board result from Search into the Boards Tree View', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'search') return { items: [searchItem('board', board.id, { board_id: null, thread_id: null, name: board.name, post_no: null })], next_cursor: null, total_count: 1 } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { host.querySelector<HTMLButtonElement>('.tree-board-toggle')!.click(); await settle() })
    expect(host.querySelector<HTMLButtonElement>('.tree-board-toggle')?.getAttribute('aria-expanded')).toBe('false')
    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    fill('[data-testid="search-query"]', 'Engineering')
    await act(async () => { testButton('search-submit').click(); await settle() })
    await act(async () => { testButton(`search-result-board-${board.id}`).click(); await settle() })
    expect(host.querySelector('[data-testid="search-dialog"]')).toBeNull()
    expect(host.querySelector<HTMLButtonElement>('.tree-board-toggle')?.getAttribute('aria-expanded')).toBe('true')
    expect(host.querySelector('[data-testid="workspace-tab-boards"]')?.getAttribute('aria-selected')).toBe('true')
    expect(host.querySelector('.tree-board-toggle.search-target-highlight')?.textContent).toContain(board.name)
    expect(host.querySelector('[data-testid="workspace-search-clear"]')).toBeNull()
    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    expect(host.querySelector<HTMLInputElement>('[data-testid="search-query"]')?.value).toBe('Engineering')
    expect(testButton('search-clear').disabled).toBe(false)
    expect(testButton('search-submit').nextElementSibling).toBe(testButton('search-clear'))
    await act(async () => { testButton('search-clear').click(); await settle() })
    expect(host.querySelector('.search-target-highlight')).toBeNull()
    expect(testButton('search-clear').disabled).toBe(true)
    await act(async () => { testButton('search-close').click(); await settle() })
    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    expect(host.querySelector<HTMLInputElement>('[data-testid="search-query"]')?.value).toBe('')
    await act(async () => { testButton('search-close').click(); await settle() })
    await act(async () => { select(threadA.name); await settle() })
    expect(host.querySelector('.search-target-highlight')).toBeNull()
  })

  it('navigates Post and Response results to their exact cross-page Post number', async () => {
    const postResult = searchItem('post', 'post-87', { post_no: 87, name: threadA.name })
    const responseResult = searchItem('response', 'response-88', { post_no: 88, name: threadA.name })
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'search') return { items: [postResult, responseResult], next_cursor: null, total_count: 2 } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.get') return post((args?.no as number), `Current body ${args?.no}` , { id: args?.no === 88 ? 'response-88' : 'post-87' }) as never
      if (operation === 'post.page') {
        const no = args?.anchor_no as number
        return postPage([post(no, `Anchored body ${no}`)], 2, 50, 100) as never
      }
      if (operation === 'response.list') return { items: [], next_before_no: null } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    fill('[data-testid="search-query"]', 'release')
    await act(async () => { testButton('search-submit').click(); await settle() })
    await act(async () => { testButton('search-result-post-post-87').click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('post.get', { thread_id: threadA.id, no: 87 })
    expect(guiRead).toHaveBeenCalledWith('post.page', expect.objectContaining({ thread_id: threadA.id, anchor_no: 87 }))
    expect(host.querySelector('[data-testid="thread-page-number-2"]')?.getAttribute('aria-current')).toBe('page')
    expect(host.querySelector('#post-87')?.textContent).toContain('Anchored body 87')
    expect(host.querySelector('#post-87')?.classList.contains('search-target-highlight')).toBe(true)

    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    fill('[data-testid="search-query"]', 'release')
    await act(async () => { testButton('search-submit').click(); await settle() })
    await act(async () => { testButton('search-result-response-response-88').click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('post.get', { thread_id: threadA.id, no: 88 })
    expect(guiRead).toHaveBeenCalledWith('post.page', expect.objectContaining({ thread_id: threadA.id, anchor_no: 88 }))
    expect(host.querySelector('#post-88')?.textContent).toContain('Anchored body 88')
    expect(host.querySelector('#post-88')?.classList.contains('search-target-highlight')).toBe(true)
    expect(host.querySelectorAll('.timeline-post.search-target-highlight')).toHaveLength(1)
    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    await act(async () => { testButton('search-clear').click(); await settle() })
    expect(host.querySelector('.search-target-highlight')).toBeNull()
    expect(host.querySelector('#post-88')?.textContent).toContain('Anchored body 88')
    await act(async () => { testButton('search-close').click(); await settle() })
    await act(async () => { select(threadA.name); await settle() })
    expect(host.querySelector('.search-target-highlight')).toBeNull()
  })

  it('keeps a parent-hidden private Thread result free of its Board metadata', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'search') return { items: [searchItem('thread', threadPrivate.id, { board_id: null, thread_id: threadPrivate.id, name: threadPrivate.name, excerpt: 'Private Thread match' })], next_cursor: null, total_count: 1 } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadPrivate] } as never
      if (operation === 'thread.get') return threadPrivate as never
      if (operation === 'post.page') return postPage([post(1, 'Private conversation')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    fill('[data-testid="search-query"]', 'private')
    await act(async () => { testButton('search-submit').click(); await settle() })
    const result = host.querySelector<HTMLElement>(`[data-testid="search-result-thread-${threadPrivate.id}"]`)
    expect(result?.textContent).toContain(threadPrivate.name)
    expect(result?.textContent).not.toContain(board.name)
    await act(async () => { result?.click(); await settle() })
    expect(host.querySelector('.thread-detail[aria-label="Private thread"]')?.textContent).toContain('Private conversation')
    expect(host.querySelector('.thread-detail[aria-label="Private thread"]')?.textContent).not.toContain(board.name)
    expect(host.querySelector('.tree-thread.search-target-highlight')?.textContent).toContain(threadPrivate.name)
    expect(host.querySelector('.thread-heading h1.search-target-highlight')?.textContent).toBe(threadPrivate.name)
    await act(async () => { select(threadPrivate.name); await settle() })
    expect(host.querySelector('.thread-heading h1.search-target-highlight')?.textContent).toBe(threadPrivate.name)
    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    fill('[data-testid="search-query"]', 'changed')
    expect(host.querySelector('.search-target-highlight')).toBeNull()
  })

  it('clears the open Thread when a search navigation target has been revoked', async () => {
    const denied: Thread = { id: 'thread-revoked', board_id: null, name: 'Revoked Thread', description: '', created_at: threadA.created_at }
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'search') return { items: [searchItem('thread', denied.id, { board_id: null, thread_id: denied.id, name: denied.name })], next_cursor: null, total_count: 1 } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get' && args?.id === denied.id) throw { code: 'NOT_FOUND', exit_code: 3, message: 'Not found' }
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, 'Previously visible conversation')]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')?.textContent).toContain('Previously visible conversation')
    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    fill('[data-testid="search-query"]', 'revoked')
    await act(async () => { testButton('search-submit').click(); await settle() })
    await act(async () => { testButton(`search-result-thread-${denied.id}`).click(); await settle() })
    expect(host.querySelector('.thread-detail')).toBeNull()
    expect(host.textContent).not.toContain('Previously visible conversation')
  })

  it('ignores a delayed search navigation after a newer result opens another Thread', async () => {
    const delayed = deferred<Thread>()
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'search') return { items: [searchItem('thread', args?.query === 'first' ? threadPrivate.id : threadA.id, { board_id: null, thread_id: args?.query === 'first' ? threadPrivate.id : threadA.id, name: args?.query === 'first' ? threadPrivate.name : threadA.name })], next_cursor: null, total_count: 1 } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA, threadPrivate] } as never
      if (operation === 'thread.get' && args?.id === threadPrivate.id) return delayed.promise as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, `Opened ${args?.thread_id}`)]) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    fill('[data-testid="search-query"]', 'first')
    await act(async () => { testButton('search-submit').click(); await settle() })
    await act(async () => { testButton(`search-result-thread-${threadPrivate.id}`).click(); await settle() })

    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    fill('[data-testid="search-query"]', 'second')
    await act(async () => { testButton('search-submit').click(); await settle() })
    await act(async () => { testButton(`search-result-thread-${threadA.id}`).click(); await settle() })
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')?.textContent).toContain(`Opened ${threadA.id}`)
    await act(async () => { delayed.resolve(threadPrivate); await delayed.promise; await settle() })
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')).not.toBeNull()
    expect(host.querySelector('.thread-detail[aria-label="Private thread"]')).toBeNull()
  })

  it('requires explicit link confirmation, supports cancel and manual retry, and restores link focus', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, '[external guide](HTTPS://Example.COM:443/docs)')]) as never
      if (operation === 'response.list') return { items: [], next_before_no: null } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    vi.mocked(guiOpenLink).mockRejectedValueOnce(new Error('shell open failed'))
    await render()
    await act(async () => { select('Release review'); await settle() })
    const link = host.querySelector<HTMLButtonElement>('[data-testid="markdown-link"]')!
    document.documentElement.style.overflow = 'auto'
    document.body.style.overflow = 'scroll'
    link.focus()
    await act(async () => { link.click(); await settle() })
    expect(host.querySelector('[data-testid="external-link-url"]')?.textContent).toBe('https://example.com/docs')
    expect(host.querySelector('[data-testid="external-link-confirmation"]')).not.toBeNull()
    expect(document.documentElement.style.overflow).toBe('hidden')
    expect(document.body.style.overflow).toBe('hidden')
    expect(guiOpenLink).not.toHaveBeenCalled()

    await act(async () => { testButton('external-link-cancel').click(); await settle() })
    expect(host.querySelector('[data-testid="external-link-confirmation"]')).toBeNull()
    expect(guiOpenLink).not.toHaveBeenCalled()
    expect(document.activeElement).toBe(link)
    expect(document.documentElement.style.overflow).toBe('auto')
    expect(document.body.style.overflow).toBe('scroll')

    await act(async () => { link.click(); await settle() })
    await act(async () => { testButton('external-link-confirm').click(); await settle() })
    expect(guiOpenLink).toHaveBeenCalledTimes(1)
    expect(guiOpenLink).toHaveBeenCalledWith('https://example.com/docs')
    expect(host.querySelector('[data-testid="external-link-error"]')?.textContent).toContain('retry')
    expect(host.querySelector('[data-testid="external-link-confirmation"]')).not.toBeNull()
    expect(document.documentElement.style.overflow).toBe('hidden')
    expect(document.body.style.overflow).toBe('hidden')
    expect(document.activeElement).toBe(testButton('external-link-confirm'))

    await act(async () => { testButton('external-link-confirm').click(); await settle() })
    expect(guiOpenLink).toHaveBeenCalledTimes(2)
    expect(host.querySelector('[data-testid="external-link-confirmation"]')).toBeNull()
    expect(document.activeElement).toBe(link)
    expect(document.documentElement.style.overflow).toBe('auto')
    expect(document.body.style.overflow).toBe('scroll')
  })

  it('invalidates a pending external link confirmation on account or authorization refresh', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage([post(1, '[external guide](https://example.com/account)')]) as never
      if (operation === 'response.list') return { items: [], next_before_no: null } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select('Release review'); await settle() })
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="markdown-link"]')!.click(); await settle() })
    expect(host.querySelector('[data-testid="external-link-confirmation"]')).not.toBeNull()
    await rerender('en', 'account-next')
    expect(host.querySelector('[data-testid="external-link-confirmation"]')).toBeNull()
    expect(guiOpenLink).not.toHaveBeenCalled()

    await act(async () => { select('Release review'); await settle() })
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="markdown-link"]')!.click(); await settle() })
    expect(host.querySelector('[data-testid="external-link-confirmation"]')).not.toBeNull()
    await rerender('en', 'account-next', 1)
    expect(host.querySelector('[data-testid="external-link-confirmation"]')).toBeNull()
    expect(guiOpenLink).not.toHaveBeenCalled()
  })

  it('defaults to Timeline and restores the selected view per account across switches and remounts', async () => {
    const threadedPosts = [post(1, 'Root', { response_count: 99 }), post(2, 'Reply', { reply_to: 1 }), post(3, 'Nested reply', { reply_to: 2 }), post(4, 'Other root')]
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage(threadedPosts) as never
      if (operation === 'response.list') return { items: [], next_before_no: null } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    mockThreadScrollMetrics()
    await render('en', 'view-account-a')
    await act(async () => { select(threadA.name); await settle() })
    expect(testButton('conversation-view-timeline').getAttribute('aria-pressed')).toBe('true')
    expect(testButton('conversation-view-threaded').getAttribute('aria-pressed')).toBe('false')
    const scroll = host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')!
    scroll.scrollTop = 173
    await act(async () => { testButton('conversation-view-threaded').click(); await settle() })
    expect(testButton('conversation-view-threaded').getAttribute('aria-pressed')).toBe('true')
    expect(window.localStorage.getItem(conversationViewPreferenceKey('view-account-a'))).toBe('threaded')
    expect(host.querySelector('[data-testid="threaded-post-1"]')).not.toBeNull()
    expect(host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')?.scrollTop).toBe(173)
    await act(async () => { testButton('threaded-collapse-1').click(); await settle() })
    const foldedScroll = host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')!
    foldedScroll.scrollTop = 247
    await act(async () => { foldedScroll.dispatchEvent(new Event('scroll', { bubbles: true })); await settle() })
    await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="Close Thread"]')!.click(); await settle() })
    await act(async () => { select(threadA.name); await settle() })
    expect(host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')?.scrollTop).toBe(247)

    await rerender('en', 'view-account-b')
    await act(async () => { select(threadA.name); await settle() })
    expect(testButton('conversation-view-timeline').getAttribute('aria-pressed')).toBe('true')
    await rerender('en', 'view-account-a')
    await act(async () => { select(threadA.name); await settle() })
    expect(window.localStorage.getItem(conversationViewPreferenceKey('view-account-a'))).toBe('threaded')
    expect(testButton('conversation-view-threaded').getAttribute('aria-pressed')).toBe('true')
    await remount('en', 'view-account-a')
    expect(window.localStorage.getItem(conversationViewPreferenceKey('view-account-a'))).toBe('threaded')
    await act(async () => { select(threadA.name); await settle() })
    expect(testButton('conversation-view-threaded').getAttribute('aria-pressed')).toBe('true')
  })

  it('renders nested replies once, collapses only a subtree, and expands ancestors for a searched Post anchor', async () => {
    const threadedPosts = [post(1, 'Root', { response_count: 99 }), post(2, 'Reply', { reply_to: 1 }), post(3, 'Nested reply', { reply_to: 2 }), post(4, 'Other root')]
    const result = searchItem('post', 'post-3', { post_no: 3, name: threadA.name })
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'search') return { items: [result], next_cursor: null, total_count: 1 } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.get') return post(args?.no as number, 'Current Post') as never
      if (operation === 'post.page') return postPage(threadedPosts) as never
      if (operation === 'response.list') return { items: [], next_before_no: null } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select(threadA.name); await settle() })
    await act(async () => { testButton('conversation-view-threaded').click(); await settle() })
    const rows = () => [...host.querySelectorAll<HTMLElement>('[data-testid^="threaded-post-"]')]
    expect(rows().map((row) => row.dataset.postNo)).toEqual(['1', '2', '3', '4'])
    expect(host.querySelector('[data-testid="threaded-post-3"]')?.getAttribute('data-depth')).toBe('2')
    expect(host.querySelector('[data-testid="threaded-post-1"]')?.getAttribute('data-descendant-count')).toBe('2')

    await act(async () => { testButton('threaded-collapse-1').click(); await settle() })
    expect(rows().map((row) => row.dataset.postNo)).toEqual(['1', '4'])
    expect(testButton('threaded-collapse-1').getAttribute('aria-label')).toMatch(/2/)
    await act(async () => { testButton('threaded-collapse-1').click(); await settle() })
    expect(rows().map((row) => row.dataset.postNo)).toEqual(['1', '2', '3', '4'])

    await act(async () => { testButton('workspace-search-open').click(); await settle() })
    fill('[data-testid="search-query"]', 'nested')
    await act(async () => { testButton('search-submit').click(); await settle() })
    await act(async () => { testButton('threaded-collapse-1').click(); await settle() })
    await act(async () => { testButton('search-result-post-post-3').click(); await settle() })
    expect(host.querySelector('[data-testid="threaded-post-1"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="threaded-post-2"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="threaded-post-3"]')).not.toBeNull()
  })

  it('loads an out-of-page reply target by No and anchors the current Thread page there', async () => {
    const current = post(61, 'Reply on this page', { reply_to: 4 })
    const parent = post(4, 'Reply target')
    vi.mocked(guiRead).mockImplementation(async (operation, args) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.get') return parent as never
      if (operation === 'post.page') {
        if (args?.anchor_no === parent.no) return postPage([post(1, 'Earlier root'), parent, post(5, 'Sibling reply', { reply_to: parent.no })], 1, 50, 90) as never
        return postPage([current], 2, 50, 90) as never
      }
      if (operation === 'response.list') return { items: [], next_before_no: null } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => { select(threadA.name); await settle() })
    expect(host.querySelector('#post-61')).not.toBeNull()
    await act(async () => { button('Reply to No. 4').click(); await settle() })
    expect(guiRead).toHaveBeenCalledWith('post.get', { thread_id: threadA.id, no: 4 })
    expect(guiRead).toHaveBeenCalledWith('post.page', expect.objectContaining({ thread_id: threadA.id, anchor_no: 4 }))
    expect(host.querySelector('#post-4')?.textContent).toContain('Reply target')
    expect(host.querySelector('[data-testid="thread-page-number-1"]')?.getAttribute('aria-current')).toBe('page')
    expect(host.querySelector('#post-61')).toBeNull()
  })

  it('promotes a saved scroll anchor to the collapsing parent when its child is hidden', async () => {
    const threadedPosts = [post(1, 'Root'), post(2, 'Reply', { reply_to: 1 }), post(3, 'Nested reply', { reply_to: 2 }), post(4, 'Other root')]
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage(threadedPosts) as never
      if (operation === 'response.list') return { items: [], next_before_no: null } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    mockThreadScrollMetrics()
    const originalRect = HTMLElement.prototype.getBoundingClientRect
    const rectSpy = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      if (this.getAttribute('data-testid') === 'thread-scroll-container') return new DOMRect(0, 0, 600, 300)
      if (this.classList.contains('thread-heading')) return new DOMRect(0, 0, 600, 40)
      if (this.id === 'post-1') return new DOMRect(0, -80, 500, 30)
      if (this.id === 'post-2') return new DOMRect(0, 55, 500, 30)
      if (this.id === 'post-3') return new DOMRect(0, 95, 500, 30)
      return originalRect.call(this)
    })
    restoreThreadRects = () => {
      rectSpy.mockRestore()
      restoreThreadRects = null
    }
    await render()
    await act(async () => { select(threadA.name); await settle() })
    await act(async () => { testButton('conversation-view-threaded').click(); await settle() })
    const scroll = host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')!
    scroll.scrollTop = 135
    await act(async () => { testButton('threaded-collapse-1').click(); await settle() })
    const saved = window.localStorage.getItem(threadScrollPreferenceKey('test-account', threadA.id))
    expect(saved).not.toBeNull()
    expect(JSON.parse(saved!).postNo).toBe(1)
    restoreThreadRects?.()
  })

  it('expands a previously folded ancestor when switching back to Threaded around the visible Post anchor', async () => {
    const threadedPosts = [post(1, 'Root'), post(2, 'Reply', { reply_to: 1 }), post(3, 'Nested reply', { reply_to: 2 }), post(4, 'Other root')]
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return editorCapabilities as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [threadA] } as never
      if (operation === 'thread.get') return threadA as never
      if (operation === 'post.page') return postPage(threadedPosts) as never
      if (operation === 'response.list') return { items: [], next_before_no: null } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    mockThreadScrollMetrics()
    const originalRect = HTMLElement.prototype.getBoundingClientRect
    const rectSpy = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      if (this.getAttribute('data-testid') === 'thread-scroll-container') return new DOMRect(0, 0, 600, 300)
      if (this.classList.contains('thread-heading')) return new DOMRect(0, 0, 600, 40)
      if (this.id === 'post-1') return new DOMRect(0, -80, 500, 30)
      if (this.id === 'post-2') return new DOMRect(0, 10, 500, 30)
      if (this.id === 'post-3') return new DOMRect(0, 65, 500, 30)
      return originalRect.call(this)
    })
    restoreThreadRects = () => {
      rectSpy.mockRestore()
      restoreThreadRects = null
    }
    await render()
    await act(async () => { select(threadA.name); await settle() })
    await act(async () => { testButton('conversation-view-threaded').click(); await settle() })
    await act(async () => { testButton('threaded-collapse-1').click(); await settle() })
    expect(host.querySelector('[data-testid="threaded-post-3"]')).toBeNull()
    await act(async () => { testButton('conversation-view-timeline').click(); await settle() })
    const scroll = host.querySelector<HTMLElement>('[data-testid="thread-scroll-container"]')!
    scroll.scrollTop = 200
    await act(async () => { scroll.dispatchEvent(new Event('scroll', { bubbles: true })); await settle() })
    expect(host.querySelector('#post-3')).not.toBeNull()
    await act(async () => { testButton('conversation-view-threaded').click(); await settle() })
    expect(host.querySelector('[data-testid="threaded-post-3"]')).not.toBeNull()
    expect(testButton('threaded-collapse-1').getAttribute('aria-expanded')).toBe('true')
    expect(JSON.parse(window.localStorage.getItem(threadScrollPreferenceKey('test-account', threadA.id))!).postNo).toBe(3)
    restoreThreadRects?.()
  })
})
