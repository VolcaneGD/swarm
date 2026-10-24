// @vitest-environment jsdom
import { act, Profiler, useRef } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import NotificationSettingsPanel from './NotificationSettingsPanel'
import useDisplayedPostRead from './useDisplayedPostRead'
import useConversationNotifications from './useConversationNotifications'
import { guiMarkDisplayedPostsRead, guiRead, guiSetNotificationSettings, guiSubscribeThread } from './bridge'

vi.mock('./bridge', () => ({ guiMarkDisplayedPostsRead: vi.fn(), guiRead: vi.fn(), guiSetNotificationSettings: vi.fn(), guiSubscribeThread: vi.fn() }))
let host: HTMLDivElement, root: Root
let observers: VisibleObserver[]
class VisibleObserver {
  bodies: Element[] = []
  constructor(private callback: IntersectionObserverCallback) { observers.push(this) }
  observe(body: Element) { this.bodies.push(body) }
  disconnect() {}
  show(indices: number[]) {
    this.callback(this.bodies.map((target, index) => ({ target, isIntersecting: indices.includes(index), intersectionRatio: indices.includes(index) ? 1 : 0 } as IntersectionObserverEntry)), this as unknown as IntersectionObserver)
  }
}
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}
function Reader({ account = 'a', thread = 't', scope = 'page-1', count = 3 }: { account?: string; thread?: string; scope?: string; count?: number }) {
  const container = useRef<HTMLDivElement>(null)
  const read = useDisplayedPostRead(account, thread, container, scope, true, () => {})
  return <div ref={container} data-testid="viewport"><header className="thread-heading" />
    {Array.from({ length: count }, (_, index) => <article key={index} className="timeline-post" data-post-id={`p-${index + 1}`}><div className="markdown-body">Post {index + 1}</div></article>)}
    <article className="timeline-post is-deleted" data-post-id="deleted"><div className="markdown-body">Deleted</div></article>
    <output>{read.failed ? 'failed' : 'ready'}</output><button onClick={read.retry}>Retry read</button>
  </div>
}
function geometry() {
  const rect = (top: number, bottom: number) => ({ top, bottom, left: 0, right: 500, width: 500, height: bottom - top, x: 0, y: top, toJSON() {} })
  host.querySelector<HTMLElement>('[data-testid="viewport"]')!.getBoundingClientRect = () => rect(0, 500)
  host.querySelector<HTMLElement>('.thread-heading')!.getBoundingClientRect = () => rect(0, 100)
  host.querySelectorAll<HTMLElement>('.markdown-body').forEach((body) => { body.getBoundingClientRect = () => rect(110, 180) })
}
async function tick(ms = 300) { await act(async () => { await vi.advanceTimersByTimeAsync(ms) }) }
async function renderReader(props = {}) { await act(async () => { root.render(<Reader {...props} />) }); geometry() }
function buttons() { return Array.from(host.querySelectorAll<HTMLButtonElement>('button')) }
beforeEach(() => {
  vi.useFakeTimers(); vi.resetAllMocks(); observers = []
  vi.stubGlobal('IntersectionObserver', VisibleObserver)
  Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' })
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  vi.mocked(guiMarkDisplayedPostsRead).mockResolvedValue({ marked: 1 })
})
afterEach(async () => { await act(async () => { root.unmount() }); host.remove(); vi.useRealTimers(); vi.unstubAllGlobals() })

describe('displayed-post read contract', () => {
  it('does not mark fetched rows until their bodies are visible, and sends sparse IDs only once', async () => {
    await renderReader(); await tick()
    expect(guiMarkDisplayedPostsRead).not.toHaveBeenCalled()
    observers[0].show([0, 2]); await tick(); await tick()
    expect(guiMarkDisplayedPostsRead).toHaveBeenCalledTimes(1)
    expect(guiMarkDisplayedPostsRead).toHaveBeenCalledWith('a', 't', ['p-1', 'p-3'])
    expect(observers[0].bodies).toHaveLength(3)
  })
  it('excludes bodies behind the sticky header or outside the viewport despite a stale observer event', async () => {
    await renderReader()
    const bodies = observers[0].bodies
    ;(bodies[0] as HTMLElement).getBoundingClientRect = () => ({ top: 0, bottom: 90, height: 90, left: 0, right: 500 } as DOMRect)
    ;(bodies[1] as HTMLElement).getBoundingClientRect = () => ({ top: 510, bottom: 600, height: 90, left: 0, right: 500 } as DOMRect)
    observers[0].show([0, 1, 2]); await tick()
    expect(guiMarkDisplayedPostsRead).toHaveBeenCalledWith('a', 't', ['p-3'])
  })
  it('keeps unread while hidden, behind a modal, or in an inert container', async () => {
    await renderReader(); observers[0].show([0])
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' }); await tick()
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' })
    const modal = document.createElement('div'); modal.setAttribute('role', 'dialog'); modal.setAttribute('aria-modal', 'true'); document.body.append(modal)
    await tick(); modal.remove(); host.setAttribute('inert', ''); await tick()
    expect(guiMarkDisplayedPostsRead).not.toHaveBeenCalled()
    host.removeAttribute('inert'); await tick()
    expect(guiMarkDisplayedPostsRead).toHaveBeenCalledTimes(1)
  })
  it('discards observer callbacks from the previous account and page', async () => {
    await renderReader(); const old = observers[0]
    await renderReader({ account: 'b', thread: 'other', scope: 'page-2' })
    old.show([0, 1]); await tick(); expect(guiMarkDisplayedPostsRead).not.toHaveBeenCalled()
    observers[1].show([2]); await tick()
    expect(guiMarkDisplayedPostsRead).toHaveBeenCalledWith('b', 'other', ['p-3'])
  })
  it('requires explicit retry after a rejected read update', async () => {
    vi.mocked(guiMarkDisplayedPostsRead).mockRejectedValueOnce(new Error('unavailable'))
    await renderReader(); observers[0].show([0]); await tick(); await tick(3000)
    expect(guiMarkDisplayedPostsRead).toHaveBeenCalledTimes(1); expect(host.textContent).toContain('failed')
    await act(async () => buttons()[0].click()); observers[1].show([0]); await tick()
    expect(guiMarkDisplayedPostsRead).toHaveBeenCalledTimes(2); expect(host.textContent).toContain('ready')
  })
  it('splits visible IDs into bounded batches without duplicates', async () => {
    await renderReader({ count: 205 }); observers[0].show(Array.from({ length: 205 }, (_, index) => index)); await tick(); await tick()
    const batches = vi.mocked(guiMarkDisplayedPostsRead).mock.calls.map((call) => call[2])
    expect(batches.map((ids) => ids.length)).toEqual([200, 5]); expect(new Set(batches.flat()).size).toBe(205)
  })
  it('does not guess visibility when the observer API is unavailable', async () => {
    vi.stubGlobal('IntersectionObserver', undefined); await renderReader(); await tick(3000)
    expect(guiMarkDisplayedPostsRead).not.toHaveBeenCalled()
  })
})

describe('notification settings contract', () => {
  async function renderSettings(accountId = 'a') {
    await act(async () => root.render(<NotificationSettingsPanel accountId={accountId} language="en" disabled={false} />))
  }
  beforeEach(() => { vi.mocked(guiRead).mockResolvedValue({ enabled: true, include_excerpt: false }) })
  it('saves only on explicit action, with the expected account and chosen preferences', async () => {
    await renderSettings(); await act(async () => host.querySelectorAll<HTMLInputElement>('input')[1].click())
    expect(guiSetNotificationSettings).not.toHaveBeenCalled()
    vi.mocked(guiSetNotificationSettings).mockResolvedValue({ enabled: true, include_excerpt: true })
    await act(async () => buttons()[0].click())
    expect(guiSetNotificationSettings).toHaveBeenCalledWith('a', { enabled: true, include_excerpt: true })
    expect(buttons()[0].disabled).toBe(true)
  })
  it('refetches after failed save without resending the mutation', async () => {
    vi.mocked(guiSetNotificationSettings).mockRejectedValue(new Error('uncertain'))
    await renderSettings(); await act(async () => host.querySelector<HTMLInputElement>('input')!.click()); await act(async () => buttons()[0].click())
    expect(buttons()[0].disabled).toBe(true); expect(host.querySelector('[role="alert"]')).not.toBeNull()
    await act(async () => buttons().find((button) => button.textContent === 'Check current settings')!.click())
    expect(guiSetNotificationSettings).toHaveBeenCalledTimes(1)
    expect(host.querySelector<HTMLInputElement>('input')!.checked).toBe(true)
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })
  it('ignores an old account save response after account selection changes', async () => {
    const pending = deferred<{ enabled: boolean; include_excerpt: boolean }>()
    vi.mocked(guiSetNotificationSettings).mockReturnValue(pending.promise)
    await renderSettings(); await act(async () => host.querySelector<HTMLInputElement>('input')!.click()); await act(async () => buttons()[0].click())
    await renderSettings('b'); await act(async () => pending.resolve({ enabled: false, include_excerpt: true }))
    expect(host.querySelector<HTMLInputElement>('input')!.checked).toBe(true)
    expect(host.querySelectorAll<HTMLInputElement>('input')[1].checked).toBe(false)
    expect(buttons()[0].textContent).toBe('Save notification settings')
  })
  it('does not expose editable defaults after the initial read fails', async () => {
    vi.mocked(guiRead).mockRejectedValue(new Error('offline')); await renderSettings()
    expect(host.querySelector('input')).toBeNull(); expect(buttons()[0].disabled).toBe(true)
  })
})

function Subscription({ token }: { token: number }) {
  const state = useConversationNotifications('a', token)
  return <button disabled={!state.ready || state.saving} aria-pressed={state.subscribed.has('t')} onClick={() => void state.toggle('t')}>Subscribe</button>
}
describe('subscription mutation contract', () => {
  it('reports read failure and recovers even when the recovered snapshot is unchanged', async () => {
    let failed = false
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (failed) throw new Error('offline')
      if (operation === 'conversation.subscriptions') return { items: [] }
      if (operation === 'conversation.unread') return { items: [], total: 0 }
      throw new Error('unexpected read')
    })
    await act(async () => root.render(<Subscription token={0} />))
    expect(buttons()[0].disabled).toBe(false)
    failed = true; await tick(5000)
    expect(buttons()[0].disabled).toBe(true)
    failed = false; await tick(5000)
    expect(buttons()[0].disabled).toBe(false)
  })

  it('polls unchanged snapshots without committing a workspace render, while applying real changes', async () => {
    let unread = 3
    let subscribed = false
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.subscriptions') return { items: subscribed ? [{ id: 't' }] : [] } as never
      if (operation === 'conversation.unread') return { items: [{ thread_id: 't', unread_count: unread }], total: unread }
      throw new Error('unexpected read')
    })
    function Snapshot() {
      const state = useConversationNotifications('a', 0)
      return <output>{state.ready ? `${state.unread.total}:${state.subscribed.has('t')}` : 'loading'}</output>
    }
    const commits = vi.fn()
    await act(async () => root.render(<Profiler id="notifications" onRender={commits}><Snapshot /></Profiler>))
    expect(host.querySelector('output')!.textContent).toBe('3:false')
    commits.mockClear(); vi.mocked(guiRead).mockClear()
    await tick(15000)
    expect(guiRead).toHaveBeenCalledTimes(6)
    expect(commits).not.toHaveBeenCalled()
    unread = 2; await tick(5000)
    expect(host.querySelector('output')!.textContent).toBe('2:false')
    expect(commits).toHaveBeenCalledTimes(1)
    subscribed = true; await tick(5000)
    expect(host.querySelector('output')!.textContent).toBe('2:true')
    expect(commits).toHaveBeenCalledTimes(2)
  })

  it('releases the button after an in-flight mutation even when a refresh token changes', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.subscriptions') return { items: [] }
      if (operation === 'conversation.unread') return { items: [], total: 0 }
      throw new Error('unexpected read')
    })
    const pending = deferred<{ id: string; subscribed: boolean }>(); vi.mocked(guiSubscribeThread).mockReturnValue(pending.promise)
    await act(async () => root.render(<Subscription token={0} />)); await act(async () => buttons()[0].click())
    await act(async () => root.render(<Subscription token={1} />)); expect(buttons()[0].disabled).toBe(true)
    await act(async () => pending.resolve({ id: 't', subscribed: true }))
    expect(buttons()[0].disabled).toBe(false)
    expect(guiSubscribeThread).toHaveBeenCalledWith('a', 't', true)
  })
})
