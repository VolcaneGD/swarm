// @vitest-environment jsdom
import { act, createRef } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'
import SearchDialog from './SearchDialog'
import { guiRead } from './bridge'
import type { Board, SearchItem, SearchPage, Thread } from './types'

vi.mock('./bridge', () => ({ guiRead: vi.fn() }))

const boards: Board[] = [
  { id: 'board-a', name: 'Engineering', description: '', created_at: '2026-10-01T00:00:00Z' },
  { id: 'board-b', name: 'Research', description: '', created_at: '2026-10-02T00:00:00Z' },
]
const threads: Thread[] = [
  { id: 'thread-a', board_id: 'board-a', name: 'Release review', description: '', created_at: '2026-10-01T00:00:00Z' },
  { id: 'thread-b', board_id: 'board-b', name: 'Search design', description: '', created_at: '2026-10-02T00:00:00Z' },
  { id: 'thread-private', board_id: null, name: 'Private thread', description: '', created_at: '2026-10-03T00:00:00Z' },
]
function item(kind: SearchItem['kind'], id: string, extra: Partial<SearchItem> = {}): SearchItem {
  return {
    kind, id, board_id: 'board-a', thread_id: 'thread-a', name: kind === 'board' || kind === 'thread' ? id : null,
    excerpt: `${kind} excerpt`, post_no: kind === 'post' || kind === 'response' ? 7 : null,
    created_at: '2026-10-08T00:00:00Z', ...extra,
  }
}
function page(items: SearchItem[], next_cursor: string | null = null): SearchPage { return { items, next_cursor, total_count: items.length } }

let host: HTMLDivElement
let root: Root
let opener: HTMLButtonElement
let openerRef: ReturnType<typeof createRef<HTMLButtonElement>>
let closed: Mock<() => void>
let navigated: Mock<(item: SearchItem) => Promise<void>>

async function settle() { await new Promise<void>((resolve) => setTimeout(resolve, 0)) }
async function render(options: { language?: 'en' | 'ja'; accountId?: string; refreshToken?: number; currentThread?: Thread | null; readableThreads?: Thread[]; readableBoards?: Board[]; busy?: boolean; creationPending?: boolean } = {}) {
  await act(async () => {
    root.render(<SearchDialog
      language={options.language || 'en'}
      accountId={options.accountId || 'account-a'}
      boards={options.readableBoards || boards}
      threads={options.readableThreads || threads}
      currentThread={options.currentThread === undefined ? threads[0] : options.currentThread}
      busy={options.busy || false}
      creationPending={options.creationPending || false}
      refreshToken={options.refreshToken || 0}
      openerRef={openerRef}
      onClose={() => closed()}
      onNavigate={(item) => navigated(item)}
      hasHighlight={false}
      onClearHighlight={() => {}}
    />)
    await settle()
  })
}
function input(testId: string, value: string) {
  const field = host.querySelector<HTMLInputElement>(`[data-testid="${testId}"]`)
  if (!field) throw new Error(`Input missing: ${testId}`)
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(field, value)
    field.dispatchEvent(new Event('input', { bubbles: true }))
  })
}
function choose(testId: string, value: string) {
  const field = host.querySelector<HTMLSelectElement>(`[data-testid="${testId}"]`)
  if (!field) throw new Error(`Select missing: ${testId}`)
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(field, value)
    field.dispatchEvent(new Event('change', { bubbles: true }))
  })
}
function range(testId: string) {
  const field = host.querySelector<HTMLInputElement>(`[data-testid="${testId}"]`)
  if (!field) throw new Error(`Range missing: ${testId}`)
  act(() => field.click())
}
function openOptionalFilters() {
  const summary = host.querySelector<HTMLElement>('[data-testid="search-optional-filters-summary"]')
  if (!summary) throw new Error('Optional search filters are missing')
  act(() => summary.click())
}
function button(testId: string) {
  const found = host.querySelector<HTMLButtonElement>(`[data-testid="${testId}"]`)
  if (!found) throw new Error(`Button missing: ${testId}`)
  return found
}
function submit() {
  const form = host.querySelector<HTMLFormElement>('form')
  if (!form) throw new Error('Search form missing')
  form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
}
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}

beforeEach(() => {
  vi.resetAllMocks()
  window.localStorage.clear()
  document.documentElement.style.overflow = ''
  document.body.style.overflow = ''
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  opener = document.createElement('button')
  opener.textContent = 'Open search'
  document.body.append(opener)
  openerRef = createRef<HTMLButtonElement>()
  openerRef.current = opener
  closed = vi.fn(() => undefined)
  navigated = vi.fn(async (_item: SearchItem) => undefined)
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
  vi.mocked(guiRead).mockResolvedValue(page([]) as never)
})

afterEach(async () => {
  if (root) await act(async () => root.unmount())
  host?.remove()
  opener?.remove()
  document.documentElement.style.overflow = ''
  document.body.style.overflow = ''
})

it('clears input filters and discards a pending search response', async () => {
  const pending = deferred<SearchPage>()
  vi.mocked(guiRead).mockReturnValueOnce(pending.promise as never)
  await render()
  input('search-query', 'needle')
  openOptionalFilters()
  input('search-project-id', 'project-id')
  input('search-author-id', 'author-id')
  input('search-from', '2026-10-01')
  input('search-to', '2026-10-10')
  await act(async () => { submit(); await settle() })
  await act(async () => { button('search-clear').click(); await settle() })
  for (const id of ['search-query', 'search-project-id', 'search-author-id', 'search-from', 'search-to']) {
    expect(host.querySelector<HTMLInputElement>(`[data-testid="${id}"]`)?.value).toBe('')
  }
  await act(async () => { pending.resolve(page([], 'old-cursor')); await settle() })
  expect(host.querySelector('[data-testid="search-results"]')).toBeNull()
  expect(host.querySelector('[data-testid="search-load-more"]')).toBeNull()
  expect(button('search-clear').disabled).toBe(true)
})

describe('Search Dialog', () => {
  it('starts with tabs, keeps its accessible name, and closes from the footer', async () => {
    await render()
    const dialog = host.querySelector<HTMLElement>('[data-testid="search-dialog"]')!
    expect(dialog.getAttribute('aria-label')).toBeTruthy()
    expect(dialog.querySelector('.search-dialog-heading, #search-dialog-title')).toBeNull()
    expect(dialog.firstElementChild?.getAttribute('role')).toBe('tablist')
    expect(dialog.lastElementChild?.tagName).toBe('FOOTER')
    expect(dialog.querySelector('footer [data-testid="search-close"]')).toBe(button('search-close'))
    await act(async () => { button('search-close').click(); await settle() })
    expect(closed).toHaveBeenCalledOnce()
  })
  it('shows the total match count rather than only the loaded page size', async () => {
    vi.mocked(guiRead).mockResolvedValueOnce({ items: [item('post', 'one')], next_cursor: 'more', total_count: 1234 } as never)
    await render()
    input('search-query', 'needle')
    await act(async () => { submit(); await settle() })
    expect(host.querySelector('[data-testid="search-result-count"]')?.textContent).toBe('Search results: 1,234')
    await act(async () => { button('search-clear').click(); await settle() })
    expect(host.querySelector('[data-testid="search-result-count"]')).toBeNull()
  })
  it('separates body, Board name and Thread name searches and ignores late results after switching', async () => {
    const pending = deferred<SearchPage>()
    vi.mocked(guiRead).mockReturnValueOnce(pending.promise as never).mockResolvedValue(page([]) as never)
    await render({ language: 'ja' })
    const tabs = [...host.querySelectorAll<HTMLButtonElement>('[role="tab"]')]
    expect(tabs.map((tab) => tab.textContent)).toEqual(['本文検索', 'ボード名検索', 'スレッド名検索'])
    input('search-query', 'needle')
    await act(async () => { submit(); await settle() })
    expect(guiRead).toHaveBeenLastCalledWith('search', expect.objectContaining({ scope: 'body' }))
    await act(async () => { tabs[1].click(); await settle() })
    expect(host.querySelector('[data-testid="search-optional-filters"]')).toBeNull()
    await act(async () => { pending.resolve(page([item('post', 'late')])); await settle() })
    expect(host.querySelector('[data-testid="search-result-post-late"]')).toBeNull()
    await act(async () => { submit(); await settle() })
    expect(guiRead).toHaveBeenLastCalledWith('search', { query: 'needle', scope: 'board_name', limit: 50 })
    await act(async () => { tabs[2].click(); await settle() })
    await act(async () => { submit(); await settle() })
    expect(guiRead).toHaveBeenLastCalledWith('search', { query: 'needle', scope: 'thread_name', limit: 50 })
  })

  it('waits for explicit submit and encodes the three location ranges and optional filters exactly', async () => {
    await render({ currentThread: threads[0] })
    input('search-query', '  release notes  ')
    expect(guiRead).not.toHaveBeenCalled()
    range('search-range-thread')
    expect(host.querySelector<HTMLSelectElement>('[data-testid="search-thread"]')?.value).toBe('thread-a')
    expect(host.querySelector<HTMLDetailsElement>('[data-testid="search-optional-filters"]')?.open).toBe(false)
    expect(host.querySelector('[data-testid="search-query"]')).not.toBeNull()
    openOptionalFilters()
    input('search-project-id', ' project-1 ')
    input('search-author-id', ' author-7 ')
    input('search-from', '2026-10-01')
    input('search-to', '2026-10-08')
    const expectedFrom = new Date(2026, 9, 1, 0, 0, 0, 0).toISOString()
    const expectedTo = new Date(2026, 9, 8, 23, 59, 59, 999).toISOString()
    await act(async () => { submit(); await settle() })
    expect(guiRead).toHaveBeenNthCalledWith(1, 'search', {
      query: 'release notes', scope: 'body', thread_id: 'thread-a', project_id: 'project-1', author_id: 'author-7',
      from: expectedFrom, to: expectedTo, limit: 50,
    })

    range('search-range-board')
    expect(host.querySelector<HTMLSelectElement>('[data-testid="search-board"]')?.value).toBe('board-a')
    await act(async () => { submit(); await settle() })
    expect(guiRead).toHaveBeenNthCalledWith(2, 'search', expect.objectContaining({ query: 'release notes', board_id: 'board-a' }))
    expect(guiRead).toHaveBeenNthCalledWith(2, 'search', expect.not.objectContaining({ thread_id: expect.anything() }))

    range('search-range-all')
    await act(async () => { submit(); await settle() })
    expect(guiRead).toHaveBeenNthCalledWith(3, 'search', { query: 'release notes', scope: 'body', project_id: 'project-1', author_id: 'author-7', from: expectedFrom, to: expectedTo, limit: 50 })
    expect(window.localStorage.length).toBe(0)
  })

  it('does not silently broaden an invalid Thread or Board range and rejects a blank query', async () => {
    await render({ currentThread: null, readableThreads: [] })
    input('search-query', 'needle')
    range('search-range-thread')
    await act(async () => { submit(); await settle() })
    expect(guiRead).not.toHaveBeenCalled()
    expect(host.querySelector('[data-testid="search-validation-error"]')?.textContent).toContain('Choose a readable Thread')

    range('search-range-board')
    choose('search-board', '')
    await act(async () => { submit(); await settle() })
    expect(guiRead).not.toHaveBeenCalled()
    expect(host.querySelector('[data-testid="search-validation-error"]')?.textContent).toContain('Choose a readable Board')

    range('search-range-all')
    input('search-query', '   ')
    await act(async () => { submit(); await settle() })
    expect(guiRead).not.toHaveBeenCalled()
    expect(host.querySelector('[data-testid="search-validation-error"]')?.textContent).toContain('Enter search terms')
  })

  it('paginates with the opaque cursor, deduplicates by kind and ID, and replaces results on a new query', async () => {
    vi.mocked(guiRead)
      .mockResolvedValueOnce(page([item('board', 'shared'), item('thread', 'shared')], 'cursor-1') as never)
      .mockResolvedValueOnce(page([item('board', 'shared'), item('post', 'post-9')], null) as never)
      .mockResolvedValueOnce(page([item('response', 'response-2')]) as never)
    await render()
    input('search-query', 'first query')
    await act(async () => { submit(); await settle() })
    expect(host.querySelectorAll('button[data-testid^="search-result-"]')).toHaveLength(2)
    expect(button('search-load-more')).not.toBeNull()
    await act(async () => { button('search-load-more').click(); await settle() })
    expect(guiRead).toHaveBeenNthCalledWith(2, 'search', { query: 'first query', scope: 'body', limit: 50, cursor: 'cursor-1' })
    expect(host.querySelectorAll('button[data-testid^="search-result-"]')).toHaveLength(3)
    expect(host.querySelector('[data-testid="search-result-board-shared"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="search-result-thread-shared"]')).not.toBeNull()

    input('search-query', 'replacement query')
    await act(async () => { submit(); await settle() })
    expect(guiRead).toHaveBeenNthCalledWith(3, 'search', { query: 'replacement query', scope: 'body', limit: 50 })
    expect(host.querySelector('[data-testid="search-result-board-shared"]')).toBeNull()
    expect(host.querySelector('[data-testid="search-result-response-response-2"]')).not.toBeNull()
  })

  it('offers retry for an initial failure and preserves rows when a later page fails', async () => {
    vi.mocked(guiRead)
      .mockRejectedValueOnce(new Error('search unavailable'))
      .mockResolvedValueOnce(page([item('post', 'post-1')], 'cursor-2') as never)
      .mockRejectedValueOnce(new Error('cursor expired'))
      .mockResolvedValueOnce(page([item('response', 'response-1')]) as never)
    await render()
    input('search-query', 'recoverable query')
    await act(async () => { submit(); await settle() })
    expect(host.querySelector('[data-testid="search-error"]')).not.toBeNull()
    await act(async () => { button('search-retry').click(); await settle() })
    expect(guiRead).toHaveBeenNthCalledWith(2, 'search', { query: 'recoverable query', scope: 'body', limit: 50 })
    expect(host.querySelector('[data-testid="search-result-post-post-1"]')).not.toBeNull()

    await act(async () => { button('search-load-more').click(); await settle() })
    expect(host.querySelector('[data-testid="search-load-more-error"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="search-result-post-post-1"]')).not.toBeNull()
    await act(async () => { button('search-load-more-retry').click(); await settle() })
    expect(guiRead).toHaveBeenNthCalledWith(4, 'search', { query: 'recoverable query', scope: 'body', limit: 50, cursor: 'cursor-2' })
    expect(host.querySelector('[data-testid="search-result-response-response-1"]')).not.toBeNull()
  })

  it('ignores a delayed result after the dialog closes and calls navigation only after close', async () => {
    const order: string[] = []
    const response = deferred<SearchPage>()
    vi.mocked(guiRead).mockReturnValue(response.promise as never)
    closed.mockImplementation(() => { order.push('close'); root.render(null) })
    await render()
    input('search-query', 'delayed')
    await act(async () => { submit(); await settle() })
    await act(async () => { button('search-close').click(); await settle() })
    await act(async () => { response.resolve(page([item('post', 'late') ])); await response.promise; await settle() })
    expect(host.querySelector('[data-testid="search-result-post-late"]')).toBeNull()
    expect(navigated).not.toHaveBeenCalled()

    const newResponse = deferred<SearchPage>()
    vi.mocked(guiRead).mockReturnValue(newResponse.promise as never)
    closed.mockClear()
    await render()
    order.length = 0
    input('search-query', 'navigate')
    await act(async () => { submit(); await settle() })
    await act(async () => { newResponse.resolve(page([item('thread', 'thread-a')])); await newResponse.promise; await settle() })
    closed.mockImplementation(() => { order.push('close'); root.render(null) })
    navigated.mockImplementation(async () => { order.push('navigate') })
    await act(async () => { button('search-result-thread-thread-a').click(); await settle() })
    expect(order).toEqual(['close', 'navigate'])
    expect(navigated).toHaveBeenCalledWith(expect.objectContaining({ kind: 'thread', id: 'thread-a' }))
  })

  it('closes and invalidates pending search when the account or permission refresh changes', async () => {
    const first = deferred<SearchPage>()
    vi.mocked(guiRead).mockReturnValue(first.promise as never)
    closed.mockImplementation(() => { root.render(null) })
    await render()
    input('search-query', 'private term')
    await act(async () => { submit(); await settle() })
    closed.mockClear()
    await render({ accountId: 'account-b' })
    await act(async () => { first.resolve(page([item('post', 'old-account') ])); await first.promise; await settle() })
    expect(host.querySelector('[data-testid="search-dialog"]')).toBeNull()
    expect(closed).toHaveBeenCalledTimes(1)
    expect(host.querySelector('[data-testid="search-result-post-old-account"]')).toBeNull()

    const second = deferred<SearchPage>()
    vi.mocked(guiRead).mockReturnValue(second.promise as never)
    closed.mockClear()
    await render()
    input('search-query', 'revoked')
    await act(async () => { submit(); await settle() })
    closed.mockClear()
    await render({ refreshToken: 1 })
    await act(async () => { second.resolve(page([item('post', 'revoked-result') ])); await second.promise; await settle() })
    expect(host.querySelector('[data-testid="search-dialog"]')).toBeNull()
    expect(closed).toHaveBeenCalledTimes(1)
    expect(host.querySelector('[data-testid="search-result-post-revoked-result"]')).toBeNull()
  })

  it('traps keyboard focus, closes on Escape, and restores focus to the search opener', async () => {
    document.documentElement.style.overflow = 'auto'
    document.body.style.overflow = 'scroll'
    closed.mockImplementation(() => { root.render(null) })
    await render({ language: 'ja' })
    expect(document.documentElement.style.overflow).toBe('hidden')
    expect(document.body.style.overflow).toBe('hidden')
    expect(document.activeElement).toBe(host.querySelector('[data-testid="search-query"]'))
    const close = button('search-close')
    const first = host.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')!
    first.focus()
    await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true })); await settle() })
    expect(document.activeElement).toBe(close)
    await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true })); await settle() })
    expect(document.activeElement).toBe(first)
    await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await settle() })
    expect(host.querySelector('[data-testid="search-dialog"]')).toBeNull()
    expect(document.activeElement).toBe(opener)
    expect(document.documentElement.style.overflow).toBe('auto')
    expect(document.body.style.overflow).toBe('scroll')
    expect(closed).toHaveBeenCalledTimes(1)
  })
})
