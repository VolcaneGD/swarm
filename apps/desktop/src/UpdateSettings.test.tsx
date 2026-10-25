// @vitest-environment jsdom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'
import UpdateSettings from './UpdateSettings'
import { checkForUpdates } from './updates'
import { getVersion } from '@tauri-apps/api/app'

vi.mock('./updates', () => ({ useStartupUpdate: () => null, checkForUpdates: vi.fn() }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn().mockResolvedValue('0.1.3') }))

it('distinguishes an unpublished private feed from being up to date and permits retry', async () => {
  const host = document.createElement('div'), root = createRoot(host)
  vi.mocked(checkForUpdates).mockRejectedValueOnce('UPDATE_UNPUBLISHED').mockResolvedValueOnce(null)
  await act(async () => root.render(<UpdateSettings language="ja" />))
  expect(host.textContent).toContain('現在のバージョン：0.1.3')
  await act(async () => host.querySelector('button')!.click())
  expect(host.textContent).toContain('公開済みの更新情報が見つかりません')
  expect(host.textContent).not.toContain('現在のバージョンは最新')
  expect(host.textContent).toContain('現在のバージョン：0.1.3')
  await act(async () => host.querySelector('button')!.click())
  expect(host.textContent).toContain('現在のバージョンは最新')
  await act(async () => root.unmount())
})

it('keeps version-read failure separate from update checking', async () => {
  vi.mocked(getVersion).mockRejectedValueOnce(new Error('unavailable'))
  const host = document.createElement('div'), root = createRoot(host)
  await act(async () => root.render(<UpdateSettings language="en" />))
  expect(host.textContent).toContain('Current version: Unavailable')
  expect(host.querySelector('button')!.disabled).toBe(false)
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
