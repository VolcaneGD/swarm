// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { guiRead } from './bridge'
import usePrivateViewGuard from './usePrivateViewGuard'
import type { ConversationCapabilities } from './types'

vi.mock('./bridge', () => ({ guiRead: vi.fn() }))
let root: Root
let container: HTMLDivElement
const lost = vi.fn()
const expired = vi.fn()
function Probe({ account = 'member', thread = 'private', capabilities }: { account?: string; thread?: string | null; capabilities?: ConversationCapabilities }) {
  usePrivateViewGuard(account, thread, lost, expired, capabilities)
  return null
}
async function focus() { await act(async () => { window.dispatchEvent(new Event('focus')) }) }
beforeEach(async () => {
  vi.useFakeTimers()
  vi.clearAllMocks()
  vi.mocked(guiRead).mockResolvedValue({ items: [], next_cursor: null })
  Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' })
  container = document.createElement('div')
  document.body.append(container)
  root = createRoot(container)
  await act(async () => { root.render(<Probe />) })
})
afterEach(async () => {
  await act(async () => { root.unmount() })
  container.remove()
  vi.useRealTimers()
})
describe('private view permission checks', () => {
  const snapshot: ConversationCapabilities = { board_create: false, boards: [], threads: [{ id: 'other', post_read: true, search: true, response_read: true, post_create: false, response_create: false, post_edit: false, post_history: true, thread_delete: false, post_delete: false, post_restore: false }] }
  it.each(['post_read', 'post_history', 'search', 'response_read'] as const)('clears cached views when another Thread loses %s', async right => {
    await act(async () => { root.render(<Probe thread={null} capabilities={snapshot} />) })
    vi.mocked(guiRead).mockResolvedValue({ ...snapshot, threads: [{ ...snapshot.threads[0], [right]: false }] })
    await focus()
    expect(lost).toHaveBeenCalledOnce()
    expect(expired).not.toHaveBeenCalled()
  })
  it('clears cached Board-name search results when only Board search is denied', async () => {
    const expected = { ...snapshot, boards: [{ id: 'board', thread_create: false, board_delete: false, search: true }] }
    await act(async () => { root.render(<Probe thread={null} capabilities={expected} />) })
    vi.mocked(guiRead).mockResolvedValue({ ...expected, boards: [{ ...expected.boards[0], search: false }] })
    await focus()
    expect(lost).toHaveBeenCalledOnce()
    expect(expired).not.toHaveBeenCalled()
  })
  it('does not discard existing views when new rights are granted', async () => {
    await act(async () => { root.render(<Probe thread={null} capabilities={snapshot} />) })
    vi.mocked(guiRead).mockResolvedValue({ ...snapshot, board_create: true })
    await focus()
    expect(lost).not.toHaveBeenCalled()
  })
  it('checks the empty body range every five seconds', async () => {
    await act(async () => { await vi.advanceTimersByTimeAsync(5000) })
    expect(guiRead).toHaveBeenCalledExactlyOnceWith('post.list', { thread_id: 'private', before_no: 1, limit: 1 })
    expect(lost).not.toHaveBeenCalled()
  })
  it.each([4, 5])('clears denied or deleted views without expiring the account (%s)', async exit_code => {
    vi.mocked(guiRead).mockRejectedValue({ exit_code })
    await focus()
    expect(lost).toHaveBeenCalledOnce()
    expect(expired).not.toHaveBeenCalled()
  })
  it('refreshes the account after credential expiry even without an open Thread', async () => {
    await act(async () => { root.render(<Probe thread={null} />) })
    vi.mocked(guiRead).mockRejectedValue({ exit_code: 3 })
    await focus()
    expect(guiRead).toHaveBeenCalledWith('conversation.capabilities')
    expect(lost).toHaveBeenCalledOnce()
    expect(expired).toHaveBeenCalledOnce()
  })
  it('retains permitted cached views on a transport failure', async () => {
    vi.mocked(guiRead).mockRejectedValue({ exit_code: 8 })
    await focus()
    expect(lost).not.toHaveBeenCalled()
  })
  it('skips hidden checks and rechecks on visibility return', async () => {
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' })
    await focus()
    expect(guiRead).not.toHaveBeenCalled()
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' })
    await act(async () => { document.dispatchEvent(new Event('visibilitychange')) })
    expect(guiRead).toHaveBeenCalledOnce()
  })
  it('allows only one check and ignores stale faults after switching accounts or Threads', async () => {
    let reject!: (reason: unknown) => void
    vi.mocked(guiRead).mockImplementation(() => new Promise((_, fail) => { reject = fail }))
    await focus()
    await focus()
    expect(guiRead).toHaveBeenCalledOnce()
    await act(async () => { root.render(<Probe account="other" thread="other-thread" />) })
    await act(async () => { reject({ exit_code: 3 }) })
    expect(lost).not.toHaveBeenCalled()
    expect(expired).not.toHaveBeenCalled()
  })
})
