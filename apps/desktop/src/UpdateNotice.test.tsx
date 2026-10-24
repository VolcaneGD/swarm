// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import UpdateNotice from './UpdateNotice'
import { installUpdate, restartAfterUpdate, type UpdateProgress } from './updates'

vi.mock('./updates', () => ({ installUpdate: vi.fn(), restartAfterUpdate: vi.fn() }))
let host: HTMLDivElement, root: Root
beforeEach(() => {
  vi.resetAllMocks()
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
})
afterEach(async () => { await act(async () => root.unmount()); host.remove() })
it('reports real progress, blocks duplicate clicks and restarts only after successful installation', async () => {
  let report!: (progress: UpdateProgress) => void
  let complete!: () => void
  vi.mocked(installUpdate).mockImplementation(callback => { report = callback; return new Promise(resolve => { complete = resolve }) })
  vi.mocked(restartAfterUpdate).mockReturnValue(new Promise(() => {}))
  await act(async () => root.render(<UpdateNotice update={{ version: '0.2.0' }} language="ja" />))
  expect(host.textContent).toContain('最新のアップデートがあります。')
  expect(host.querySelector('progress')).toBeNull()
  await act(async () => host.querySelector('button')!.click())
  expect(host.querySelector('button')!.disabled).toBe(true)
  await act(async () => report({ phase: 'downloading', downloaded: 25, total: 100 }))
  expect(host.querySelector('progress')!.value).toBe(25)
  expect(host.textContent).toContain('25%')
  expect(restartAfterUpdate).not.toHaveBeenCalled()
  await act(async () => report({ phase: 'verifying' }))
  expect(host.querySelector('progress')!.hasAttribute('value')).toBe(false)
  await act(async () => complete())
  expect(restartAfterUpdate).toHaveBeenCalledTimes(1)
  expect(host.textContent).toContain('アップデート完了。再起動しています')
})
it('never restarts on failed installation and allows a retry', async () => {
  vi.mocked(installUpdate).mockRejectedValue('download failed')
  await act(async () => root.render(<UpdateNotice update={{ version: '0.2.0' }} language="en" />))
  await act(async () => host.querySelector('button')!.click())
  expect(restartAfterUpdate).not.toHaveBeenCalled()
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('Could not start')
  expect(host.querySelector('button')!.disabled).toBe(false)
  expect(host.querySelector('progress')).toBeNull()
})
it('explains the placeholder delivery gate without claiming completion', async () => {
  vi.mocked(installUpdate).mockRejectedValue('UPDATE_NOT_CONFIGURED')
  await act(async () => root.render(<UpdateNotice update={{ version: '0.2.0' }} language="ja" />))
  await act(async () => host.querySelector('button')!.click())
  expect(host.textContent).toContain('配信設定は公開後に有効')
  expect(restartAfterUpdate).not.toHaveBeenCalled()
})
