// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import useDisplaySettings from './useDisplaySettings'
import { guiDisplayDefaults, guiDisplaySettings, guiSetDisplaySettings } from './bridge'
import type { DisplaySettings } from './types'
vi.mock('./bridge', () => ({ guiDisplayDefaults: vi.fn(), guiDisplaySettings: vi.fn(), guiSetDisplaySettings: vi.fn() }))
let root: Root, host: HTMLDivElement
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done }); return { resolve, promise } }
function Preferences({ account }: { account: string | null }) {
  const state = useDisplaySettings(account)
  return <div><output>{JSON.stringify(state.settings)}</output><span>{state.failed ? 'failed' : ''}</span>
    <button disabled={!state.ready || state.saving} onClick={() => void state.change({ language: 'ja' })}>Japanese</button>
    <button disabled={state.saving} onClick={() => void state.refresh()}>Check</button></div>
}
async function render(account: string | null) { await act(async () => { root.render(<Preferences account={account} />) }) }
beforeEach(() => {
  vi.resetAllMocks(); host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  vi.mocked(guiDisplayDefaults).mockResolvedValue({ language: 'en', theme: 'dark' })
  vi.mocked(guiDisplaySettings).mockResolvedValue({ settings: null })
  vi.mocked(guiSetDisplaySettings).mockImplementation(async (_account, value) => value)
})
afterEach(async () => { await act(async () => root.unmount()); host.remove() })
describe('account display preferences', () => {
  it('uses OS defaults only when no account preference is saved, without an implicit write', async () => {
    await render('a'); expect(host.querySelector('output')?.textContent).toBe('{"language":"en","theme":"dark"}')
    expect(guiSetDisplaySettings).not.toHaveBeenCalled()
    vi.mocked(guiDisplaySettings).mockResolvedValue({ settings: { language: 'ja', theme: 'light' } })
    vi.mocked(guiDisplayDefaults).mockRejectedValue(new Error('OS unavailable'))
    await render('b'); expect(host.querySelector('output')?.textContent).toBe('{"language":"ja","theme":"light"}')
    expect(host.textContent).not.toContain('failed')
  })
  it('saves chosen language with the current theme and expected account', async () => {
    await render('a'); await act(async () => host.querySelector<HTMLButtonElement>('button')!.click())
    expect(guiSetDisplaySettings).toHaveBeenCalledWith('a', { language: 'ja', theme: 'dark' })
    expect(host.querySelector('output')?.textContent).toContain('"language":"ja"')
  })
  it('keeps guest choices temporary and resets them when an account is selected', async () => {
    await render(null); await act(async () => host.querySelector<HTMLButtonElement>('button')!.click())
    expect(guiSetDisplaySettings).not.toHaveBeenCalled()
    await render('a'); expect(host.querySelector('output')?.textContent).toContain('"language":"en"')
  })
  it('rejects stale account save and load responses', async () => {
    const saving = deferred<DisplaySettings>(); vi.mocked(guiSetDisplaySettings).mockReturnValue(saving.promise)
    await render('a'); await act(async () => host.querySelector<HTMLButtonElement>('button')!.click())
    vi.mocked(guiDisplaySettings).mockResolvedValue({ settings: { language: 'en', theme: 'light' } })
    await render('b'); await act(async () => saving.resolve({ language: 'ja', theme: 'dark' }))
    expect(host.querySelector('output')?.textContent).toBe('{"language":"en","theme":"light"}')
    const reading = deferred<{ settings: DisplaySettings | null }>(); vi.mocked(guiDisplaySettings).mockReturnValueOnce(reading.promise)
    await render('c'); await render('d'); await act(async () => reading.resolve({ settings: { language: 'ja', theme: 'dark' } }))
    expect(host.querySelector('output')?.textContent).toBe('{"language":"en","theme":"light"}')
  })
  it('stops further changes after an uncertain save, and refetches without resending', async () => {
    vi.mocked(guiSetDisplaySettings).mockRejectedValue(new Error('uncertain'))
    await render('a'); await act(async () => host.querySelector<HTMLButtonElement>('button')!.click())
    expect(host.textContent).toContain('failed'); expect(host.querySelector<HTMLButtonElement>('button')!.disabled).toBe(true)
    await act(async () => host.querySelectorAll<HTMLButtonElement>('button')[1].click())
    expect(guiSetDisplaySettings).toHaveBeenCalledTimes(1); expect(host.querySelector<HTMLButtonElement>('button')!.disabled).toBe(false)
  })
  it('does not replace unreadable saved preferences with defaults or enable mutation', async () => {
    vi.mocked(guiDisplaySettings).mockRejectedValue(new Error('corrupt'))
    await render('a'); expect(host.querySelector<HTMLButtonElement>('button')!.disabled).toBe(true)
    expect(guiDisplayDefaults).not.toHaveBeenCalled(); expect(guiSetDisplaySettings).not.toHaveBeenCalled()
  })
  it.each([{}, { settings: false }])('rejects malformed read results rather than treating them as unsaved: %j', async (value) => {
    vi.mocked(guiDisplaySettings).mockResolvedValue(value as unknown as { settings: DisplaySettings | null })
    await render('a'); expect(host.textContent).toContain('failed')
    expect(guiDisplayDefaults).not.toHaveBeenCalled(); expect(guiSetDisplaySettings).not.toHaveBeenCalled()
  })
})
