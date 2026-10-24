import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushRegisteredPreferences, installNativeClosePreferenceListener, registerPreferenceFlush } from './nativeClosePreferences'

const { invokeMock, listenMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), listenMock: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }))

let unregister: (() => void)[] = []
afterEach(() => {
  unregister.forEach((remove) => remove())
  unregister = []
  vi.clearAllMocks()
})

describe('native close preference handshake', () => {
  it('does not register a native event listener outside the desktop host', async () => {
    const stop = await installNativeClosePreferenceListener(false)
    stop()
    expect(listenMock).not.toHaveBeenCalled()
  })

  it('flushes registered writers before acknowledging close', async () => {
    const flush = vi.fn(async () => undefined)
    unregister.push(registerPreferenceFlush(flush))
    let handler!: (event: { payload: { request_id: number } }) => Promise<void>
    listenMock.mockImplementation(async (_event: string, callback: typeof handler) => {
      handler = callback
      return vi.fn()
    })
    const stop = await installNativeClosePreferenceListener(true)
    await handler({ payload: { request_id: 42 } })
    expect(flush).toHaveBeenCalledOnce()
    expect(invokeMock).toHaveBeenCalledWith('gui_close_ready', { requestId: 42, success: true })
    stop()
  })

  it('acknowledges failure when any registered writer cannot confirm its save', async () => {
    unregister.push(registerPreferenceFlush(async () => { throw new Error('write failed') }))
    let handler!: (event: { payload: { request_id: number } }) => Promise<void>
    listenMock.mockImplementation(async (_event: string, callback: typeof handler) => {
      handler = callback
      return vi.fn()
    })
    const stop = await installNativeClosePreferenceListener(true)
    await handler({ payload: { request_id: 43 } })
    expect(invokeMock).toHaveBeenCalledWith('gui_close_ready', { requestId: 43, success: false })
    stop()
  })

  it('waits for all writers and rejects if one fails', async () => {
    const complete = vi.fn()
    unregister.push(registerPreferenceFlush(async () => { await Promise.resolve(); complete() }))
    unregister.push(registerPreferenceFlush(async () => { throw new Error('write failed') }))
    await expect(flushRegisteredPreferences()).rejects.toThrow(/could not be confirmed/i)
    expect(complete).toHaveBeenCalledOnce()
  })
})
