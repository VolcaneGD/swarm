// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import RetentionSettingsPanel from './RetentionSettingsPanel'
import { guiRetentionSettings, guiSetRetentionSettings, type RetentionSettings } from './bridge'
vi.mock('./bridge', () => ({ guiRetentionSettings: vi.fn(), guiSetRetentionSettings: vi.fn() }))
let host: HTMLDivElement, root: Root
const indefinite = { history_days: null, deleted_days: null, deleted_board_days: null, deleted_thread_days: null, archived_thread_days: null }
beforeEach(() => { vi.resetAllMocks(); host = document.createElement('div'); document.body.append(host); root = createRoot(host); vi.mocked(guiRetentionSettings).mockResolvedValue(indefinite) })
afterEach(async () => { await act(async () => root.unmount()); host.remove() })
const render = (accountId = 'admin') => act(async () => root.render(<RetentionSettingsPanel accountId={accountId} language="en" disabled={false} />))
const button = (text: string) => Array.from(host.querySelectorAll<HTMLButtonElement>('button')).find(value => value.textContent === text)!
async function input(index: number, value: string) {
  await act(async () => {
    const field = host.querySelectorAll<HTMLInputElement>('input')[index]
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(field, value)
    field.dispatchEvent(new Event('input', { bubbles: true }))
  })
}
it('defaults to indefinite, rejects invalid days and saves only an explicit valid change', async () => {
  await render()
  expect(Array.from(host.querySelectorAll<HTMLInputElement>('input')).map(value => value.value)).toEqual(['', '', '', '', ''])
  expect(guiSetRetentionSettings).not.toHaveBeenCalled()
  for (const value of ['0', '-1', '1.5', '9007199254740992']) {
    await input(0, value)
    expect(button('Save retention settings').disabled).toBe(true)
  }
  await input(0, '30')
  vi.mocked(guiSetRetentionSettings).mockResolvedValue({ history_days: 30, deleted_days: null })
  await act(async () => button('Save retention settings').click())
  expect(guiSetRetentionSettings).toHaveBeenCalledWith('admin', { history_days: 30, deleted_days: null, deleted_board_days: null, deleted_thread_days: null, archived_thread_days: null })
  expect(button('Save retention settings').disabled).toBe(true)
})
it('validates and saves the deleted Board, deleted Thread, and archived Thread periods', async () => {
  await render()
  await input(2, '90')
  await input(3, '0')
  expect(button('Save retention settings').disabled).toBe(true)
  await input(3, '30')
  await input(4, '7')
  vi.mocked(guiSetRetentionSettings).mockResolvedValue({ ...indefinite, deleted_board_days: 90, deleted_thread_days: 30, archived_thread_days: 7 })
  await act(async () => button('Save retention settings').click())
  expect(guiSetRetentionSettings).toHaveBeenCalledWith('admin', { history_days: null, deleted_days: null, deleted_board_days: 90, deleted_thread_days: 30, archived_thread_days: 7 })
})
it('omits retention fields absent from an older server response when saving other settings', async () => {
  vi.mocked(guiRetentionSettings).mockResolvedValue({ history_days: null, deleted_days: null })
  await render()
  await input(0, '14')
  vi.mocked(guiSetRetentionSettings).mockResolvedValue({ history_days: 14, deleted_days: null })
  await act(async () => button('Save retention settings').click())
  expect(guiSetRetentionSettings).toHaveBeenCalledWith('admin', { history_days: 14, deleted_days: null })
})
it('blocks repeated saves and requires readback after an uncertain save', async () => {
  await render(); await input(1, '7')
  let reject!: (error: Error) => void
  vi.mocked(guiSetRetentionSettings).mockReturnValue(new Promise((_, fail) => { reject = fail }))
  await act(async () => { button('Save retention settings').click(); button('Save retention settings').click() })
  expect(guiSetRetentionSettings).toHaveBeenCalledTimes(1)
  await act(async () => reject(new Error('lost response')))
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('Save result is uncertain')
  expect(button('Save retention settings').disabled).toBe(true)
  vi.mocked(guiRetentionSettings).mockResolvedValue({ history_days: null, deleted_days: 7 })
  await act(async () => button('Check current settings').click())
  expect(host.querySelectorAll<HTMLInputElement>('input')[1].value).toBe('7')
  expect(host.querySelector('[role="alert"]')).toBeNull()
})
it('discards responses from the previous account', async () => {
  let resolve!: (value: RetentionSettings) => void
  vi.mocked(guiRetentionSettings).mockReturnValueOnce(new Promise(done => { resolve = done })).mockResolvedValue({ history_days: 14, deleted_days: null })
  await render('old'); await render('new')
  await act(async () => resolve({ history_days: 1, deleted_days: 1 }))
  expect(host.querySelectorAll<HTMLInputElement>('input')[0].value).toBe('14')
  expect(host.querySelectorAll<HTMLInputElement>('input')[1].value).toBe('')
})
it('shows permission denial without exposing editable settings', async () => {
  vi.mocked(guiRetentionSettings).mockRejectedValue({ code: 'PERMISSION_DENIED' })
  await render()
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('permission')
  expect(host.querySelector('input')).toBeNull()
  expect(button('Save retention settings').disabled).toBe(true)
})
