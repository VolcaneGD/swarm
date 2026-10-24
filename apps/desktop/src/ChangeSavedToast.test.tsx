// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import ChangeSavedToast from './ChangeSavedToast'
import { clearOperationStatuses, trackOperation } from './operationStatus'
import { announcePasswordChange } from './changeFeedback'
let host: HTMLDivElement
let root: Root
beforeEach(async () => {
  vi.useFakeTimers(); clearOperationStatuses()
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  await act(async () => root.render(<ChangeSavedToast language="ja" />))
})
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.useRealTimers() })
it('announces a confirmed password change explicitly and keeps credentials out of the toast', async () => {
  await act(async () => { await trackOperation('gui_set_password', async () => undefined) })
  expect(host.textContent).toBe('')
  await act(async () => announcePasswordChange('changed'))
  expect(host.querySelector('[role="status"]')?.textContent).toBe('パスワードを変更しました')
  await act(async () => vi.advanceTimersByTime(3500))
  expect(host.textContent).toBe('パスワードを変更しました')
  await act(async () => vi.advanceTimersByTime(500))
  expect(host.textContent).toBe('')
})
it('announces successful changes, extends on another save and automatically disappears', async () => {
  await act(async () => { await trackOperation('rules.save', async () => undefined) })
  expect(host.textContent).toBe('変更を保存しました')
  await act(async () => vi.advanceTimersByTime(1500))
  await act(async () => { await trackOperation('notification.settings.set', async () => undefined) })
  await act(async () => vi.advanceTimersByTime(1500))
  expect(host.textContent).toBe('変更を保存しました')
  await act(async () => vi.advanceTimersByTime(500))
  expect(host.textContent).toBe('')
})
it('keeps reads silent and reports failed or partial saves in a red toast', async () => {
  await act(async () => {
    await trackOperation('thread.list', async () => undefined)
    await trackOperation('post.read.mark', async () => undefined)
    await trackOperation('permission.set', async () => undefined)
  })
  expect(host.textContent).toBe('')
  await act(async () => { await expect(trackOperation('permission.batch.save', async () => { throw Error('partial batch') })).rejects.toThrow() })
  expect(host.querySelector('.guidance-toast')?.textContent).toBe('保存に失敗しました')
  expect(host.querySelector('[data-testid="change-saved-toast"]')).toBeNull()
})
it('explains disabled controls without running an operation and replaces rather than stacks toasts', async () => {
  const button = document.createElement('button'); button.disabled = true; button.dataset.disabledReason = '先に下書きを検証してください'; host.append(button)
  await act(async () => button.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 })))
  expect(host.querySelector('.guidance-toast')?.textContent).toBe('先に下書きを検証してください')
  expect(host.querySelectorAll('.change-saved-toast')).toHaveLength(1)
  await act(async () => { await trackOperation('rules.save', async () => undefined) })
  expect(host.querySelector('.guidance-toast')).toBeNull()
  expect(host.querySelector('[data-testid="change-saved-toast"]')?.textContent).toBe('変更を保存しました')
  button.remove()
})
it('handles a disabled toggle label and an accessible aria-disabled keyboard control', async () => {
  const label = document.createElement('label')
  const input = document.createElement('input'); input.type = 'radio'; input.disabled = true; input.dataset.disabledReason = '対象を選択してください'
  const text = document.createElement('span'); text.textContent = '許可'; label.append(input, text); document.body.append(label)
  await act(async () => text.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 })))
  expect(host.querySelector('.guidance-toast')?.textContent).toBe('対象を選択してください')
  const button = document.createElement('button'); button.setAttribute('aria-disabled','true'); button.dataset.disabledReason = '権限がありません'; document.body.append(button)
  await act(async () => button.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })))
  expect(host.querySelector('.guidance-toast')?.textContent).toBe('権限がありません')
  label.remove(); button.remove()
})
