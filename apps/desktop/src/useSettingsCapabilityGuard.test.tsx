// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import useSettingsCapabilityGuard from './useSettingsCapabilityGuard'
import { guiRead } from './bridge'

vi.mock('./bridge', () => ({ guiRead: vi.fn() }))
const lost = vi.fn()
let host: HTMLDivElement
let root: Root
const rights = { account_list: true, account_get: true }
function Probe({ account = 'a', expected = rights, operation = 'management.capabilities' }: {
  account?: string; expected?: object; operation?: 'management.capabilities' | 'authorization.capabilities' | 'rules.capabilities'
}) { useSettingsCapabilityGuard(account, operation, expected, lost); return null }
async function render(props: Parameters<typeof Probe>[0] = {}) { await act(async () => root.render(<Probe {...props} />)) }
async function focus() { await act(async () => { window.dispatchEvent(new Event('focus')) }) }
beforeEach(async () => {
  vi.useFakeTimers(); vi.clearAllMocks()
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
  Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true })
  vi.mocked(guiRead).mockResolvedValue(rights as never)
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  await render()
})
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.useRealTimers() })

it('revalidates visible settings every five seconds and invalidates only once', async () => {
  vi.mocked(guiRead).mockResolvedValue({ ...rights, account_list: false } as never)
  await act(async () => { await vi.advanceTimersByTimeAsync(5000) })
  expect(lost).toHaveBeenCalledOnce()
  await focus()
  expect(guiRead).toHaveBeenCalledOnce()
})
it('ignores new grants and temporary connection failures', async () => {
  vi.mocked(guiRead).mockResolvedValue({ ...rights, account_create: true } as never)
  await focus(); expect(lost).not.toHaveBeenCalled()
  vi.mocked(guiRead).mockRejectedValue({ exit_code: 8 })
  await focus(); expect(lost).not.toHaveBeenCalled()
})
it.each([3, 4, 5])('clears private settings on authoritative exit %i', async exit_code => {
  vi.mocked(guiRead).mockRejectedValue({ exit_code })
  await focus(); expect(lost).toHaveBeenCalledOnce()
})
it('detects an operation scope being removed without a boolean change', async () => {
  await render({ operation: 'authorization.capabilities', expected: { permission_set: true, operations: [{ operation: 'post.get', scopes: ['global','thread'] }] } })
  vi.mocked(guiRead).mockResolvedValue({ permission_set: true, operations: [{ operation: 'post.get', scopes: ['global'] }] } as never)
  await focus(); expect(lost).toHaveBeenCalledOnce()
})
it('detects a project disappearing from readable rule scopes', async () => {
  await render({ operation: 'rules.capabilities', expected: { global: { read: true }, projects: [{ id: 'p', name: 'P', read: true }] } })
  vi.mocked(guiRead).mockResolvedValue({ global: { read: true }, projects: [] } as never)
  await focus(); expect(lost).toHaveBeenCalledOnce()
})
it('does not confuse project renames or grant ordering with access loss', async () => {
  await render({ operation: 'rules.capabilities', expected: { global: { read: true }, projects: [{ id: 'p', name: 'Old', read: true }], actions: ['read', 'save'] } })
  vi.mocked(guiRead).mockResolvedValue({ global: { read: true }, projects: [{ id: 'p', name: 'New', read: true }], actions: ['save', 'read', 'validate'] } as never)
  await focus(); expect(lost).not.toHaveBeenCalled()
})
it('skips hidden documents and checks on visibility return', async () => {
  Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true })
  await focus(); expect(guiRead).not.toHaveBeenCalled()
  vi.mocked(guiRead).mockResolvedValue({ account_list: false } as never)
  Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true })
  await act(async () => { document.dispatchEvent(new Event('visibilitychange')) })
  expect(lost).toHaveBeenCalledOnce()
})
it.each(['account', 'snapshot'])('ignores late denial after the %s changes', async change => {
  let reject!: (reason: unknown) => void
  vi.mocked(guiRead).mockImplementationOnce(() => new Promise((_resolve, fail) => { reject = fail }))
  await focus()
  await render(change === 'account' ? { account: 'b' } : { expected: { ...rights } })
  await act(async () => { reject({ exit_code: 3 }) })
  expect(lost).not.toHaveBeenCalled()
})
it('deduplicates checks while a probe is pending and ignores completion after unmount', async () => {
  let resolve!: (value: never) => void
  vi.mocked(guiRead).mockImplementationOnce(() => new Promise(done => { resolve = done }))
  await focus(); await focus(); expect(guiRead).toHaveBeenCalledOnce()
  await act(async () => root.render(null))
  await act(async () => { resolve({ account_list: false } as never) })
  expect(lost).not.toHaveBeenCalled()
})
