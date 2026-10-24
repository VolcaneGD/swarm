// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import UpdateNotice from './UpdateNotice'
import { installUpdate, type UpdateProgress } from './updates'
import { registerUpdateBlocker } from './nativeClosePreferences'

vi.mock('./updates', () => ({ installUpdate: vi.fn() }))
let host: HTMLDivElement, root: Root
beforeEach(() => {
  vi.resetAllMocks()
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
})
afterEach(async () => { await act(async () => root.unmount()); host.remove() })
it('requires confirmation, reports real progress and blocks duplicate installs', async () => {
  let report!: (progress: UpdateProgress) => void
  let complete!: () => void
  vi.mocked(installUpdate).mockImplementation(callback => { report = callback; return new Promise(resolve => { complete = resolve }) })
  await act(async () => root.render(<UpdateNotice update={{ version: '0.2.0' }} language="ja" />))
  expect(host.textContent).toContain('最新のアップデートがあります。')
  expect(host.querySelector('progress')).toBeNull()
  await act(async () => host.querySelector('button')!.click())
  expect(installUpdate).not.toHaveBeenCalled()
  expect(host.textContent).toContain('未送信の投稿を保存')
  await act(async () => host.querySelector('button')!.click())
  expect(host.querySelector('button')!.disabled).toBe(true)
  await act(async () => report({ phase: 'downloading', downloaded: 25, total: 100 }))
  expect(host.querySelector('progress')!.value).toBe(25)
  expect(host.textContent).toContain('25%')
  await act(async () => report({ phase: 'verifying' }))
  expect(host.querySelector('progress')!.hasAttribute('value')).toBe(false)
  await act(async () => complete())
  expect(host.textContent).toContain('アップデート完了。再起動しています')
})
it('never restarts on failed installation and allows a retry', async () => {
  vi.mocked(installUpdate).mockRejectedValue('download failed')
  await act(async () => root.render(<UpdateNotice update={{ version: '0.2.0' }} language="en" />))
  await act(async () => host.querySelector('button')!.click())
  await act(async () => host.querySelector('button')!.click())
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('Could not start')
  expect(host.querySelector('button')!.disabled).toBe(false)
  expect(host.querySelector('progress')).toBeNull()
})
it('explains installer-only updates without claiming completion', async () => {
  vi.mocked(installUpdate).mockRejectedValue('UPDATE_INSTALLATION_REQUIRED')
  await act(async () => root.render(<UpdateNotice update={{ version: '0.2.0' }} language="ja" />))
  await act(async () => host.querySelector('button')!.click())
  await act(async () => host.querySelector('button')!.click())
  expect(host.textContent).toContain('インストーラーで導入したSwarm')
})

it('cancel keeps the app running and never downloads or installs', async () => {
  await act(async () => root.render(<UpdateNotice update={{ version: '0.2.0' }} language="en" />))
  await act(async () => host.querySelector('button')!.click())
  await act(async () => host.querySelectorAll('button')[1].click())
  expect(installUpdate).not.toHaveBeenCalled()
  expect(host.textContent).not.toContain('Save unsent')
})

it('keeps unsent drafts in place and never starts an installer while editing', async () => {
  const unregister = registerUpdateBlocker(() => true)
  try {
    await act(async () => root.render(<UpdateNotice update={{ version: '0.2.0' }} language="ja" />))
    await act(async () => host.querySelector('button')!.click())
    await act(async () => host.querySelector('button')!.click())
    expect(installUpdate).not.toHaveBeenCalled()
    expect(host.textContent).toContain('未送信の投稿または編集中')
  } finally { unregister() }
})
