// @vitest-environment jsdom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'
import UpdateSettings from './UpdateSettings'
import { checkForUpdates } from './updates'

vi.mock('./updates', () => ({ useStartupUpdate: () => null, checkForUpdates: vi.fn() }))

it('distinguishes an unpublished private feed from being up to date and permits retry', async () => {
  const host = document.createElement('div'), root = createRoot(host)
  vi.mocked(checkForUpdates).mockRejectedValueOnce('UPDATE_UNPUBLISHED').mockResolvedValueOnce(null)
  await act(async () => root.render(<UpdateSettings language="ja" />))
  await act(async () => host.querySelector('button')!.click())
  expect(host.textContent).toContain('公開済みの更新情報が見つかりません')
  expect(host.textContent).not.toContain('現在のバージョンは最新')
  await act(async () => host.querySelector('button')!.click())
  expect(host.textContent).toContain('現在のバージョンは最新')
  await act(async () => root.unmount())
})

it('disables checks while the request is pending and reports transport errors', async () => {
  const host = document.createElement('div'), root = createRoot(host)
  let reject!: (error: Error) => void
  vi.mocked(checkForUpdates).mockReturnValueOnce(new Promise((_, fail) => { reject = fail }))
  await act(async () => root.render(<UpdateSettings language="en" />))
  await act(async () => host.querySelector('button')!.click())
  expect(host.querySelector('button')!.disabled).toBe(true)
  await act(async () => reject(new Error('network failure')))
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('Could not check')
  expect(host.querySelector('button')!.disabled).toBe(false)
  await act(async () => root.unmount())
})
