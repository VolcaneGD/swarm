// @vitest-environment jsdom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'
import AccountNameInput from './AccountNameInput'

it('offers bounded unique names without pre-filling a name or a password', async () => {
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
  const host = document.createElement('div'); document.body.append(host)
  const root = createRoot(host)
  const change = vi.fn()
  await act(async () => root.render(<label>Account name<AccountNameInput history={['Admin', 'メンバー', 'Admin', '']} value="" onChange={change} disabled /></label>))
  const input = host.querySelector('input')!
  expect(input.value).toBe('')
  expect(input.disabled).toBe(true)
  expect(input.getAttribute('list')).toBe(host.querySelector('datalist')!.id)
  expect([...host.querySelectorAll('option')].map(option => option.value)).toEqual(['Admin', 'メンバー'])
  expect(host.querySelector('input[type="password"]')).toBeNull()
  await act(async () => root.unmount()); host.remove()
})
