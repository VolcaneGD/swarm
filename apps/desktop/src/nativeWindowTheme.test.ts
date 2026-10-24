// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { setTheme } = vi.hoisted(() => ({ setTheme: vi.fn() }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ setTheme }) }))

beforeEach(() => { vi.resetModules(); setTheme.mockReset(); setTheme.mockResolvedValue(undefined) })
afterEach(() => { delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ })

describe('native theme synchronization', () => {
  it('skips browser preview IPC mocks that have no native window', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true })
    const { applyNativeWindowTheme } = await import('./useNativeWindowTheme')
    await applyNativeWindowTheme('dark')
    expect(setTheme).not.toHaveBeenCalled()
  })

  it('applies the selected theme to a real native window binding', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: { metadata: { currentWindow: { label: 'main' } } }, configurable: true })
    const { applyNativeWindowTheme } = await import('./useNativeWindowTheme')
    await applyNativeWindowTheme('dark')
    expect(setTheme).toHaveBeenCalledWith('dark')
  })

  it('does not invoke desktop APIs in a browser', async () => {
    const { applyNativeWindowTheme } = await import('./useNativeWindowTheme')
    await applyNativeWindowTheme('dark', false)
    expect(setTheme).not.toHaveBeenCalled()
  })

  it('keeps the last requested theme after overlapping updates', async () => {
    const { applyNativeWindowTheme } = await import('./useNativeWindowTheme')
    let finish!: () => void
    setTheme.mockImplementationOnce(() => new Promise<void>(resolve => { finish = resolve }))
    const dark = applyNativeWindowTheme('dark', true)
    const light = applyNativeWindowTheme('light', true)
    await vi.waitFor(() => expect(setTheme).toHaveBeenCalledTimes(1))
    expect(setTheme).toHaveBeenLastCalledWith('dark')
    finish()
    await Promise.all([dark, light])
    expect(setTheme.mock.calls.map(call => call[0])).toEqual(['dark', 'light'])
  })

  it('reports a native failure and permits a later successful retry', async () => {
    const { applyNativeWindowTheme } = await import('./useNativeWindowTheme')
    setTheme.mockRejectedValueOnce(new Error('Native failure'))
    await expect(applyNativeWindowTheme('dark', true)).rejects.toThrow('Native failure')
    await expect(applyNativeWindowTheme('dark', true)).resolves.toBeUndefined()
    expect(setTheme).toHaveBeenCalledTimes(2)
  })
})
