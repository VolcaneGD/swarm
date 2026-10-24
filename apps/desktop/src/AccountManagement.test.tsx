// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'
import AccountManagement from './AccountManagement'
import { guiAccountManage, guiCreationStatus, guiRead } from './bridge'
import type { Account, CreationStatus, ManagementCapabilities } from './types'

vi.mock('./bridge', () => ({
  guiRead: vi.fn(),
  guiCreationStatus: vi.fn(),
  guiAccountManage: vi.fn(),
}))

const admin: Account = { id: 'admin-1', name: 'Main administrator', is_admin: true, disabled: false, role_ids: [] }
const member: Account = { id: 'member-1', name: 'Rin', is_admin: false, disabled: false, role_ids: ['role-reader'] }
const disabledMember: Account = { ...member, id: 'member-disabled', name: 'Mina', disabled: true }
const fullCapabilities: ManagementCapabilities = {
  account_list: true, account_get: true, account_create: true, account_disable: true, account_reset: true,
}
const noCapabilities: ManagementCapabilities = {
  account_list: false, account_get: false, account_create: false, account_disable: false, account_reset: false,
}
const noWritesCapabilities: ManagementCapabilities = { ...fullCapabilities, account_create: false, account_disable: false, account_reset: false }
const readyCreation: CreationStatus = { pending: false, operation: null, expired: false }

let host: HTMLDivElement
let root: Root
let currentAccountId = 'admin-1'
let onAccountChanged: Mock<() => Promise<void>>
let onBusyChange: Mock<(busy: boolean) => void>

async function settle() { await new Promise<void>((resolve) => setTimeout(resolve, 0)) }
function render(language: 'en' | 'ja' = 'en') {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  return act(async () => {
    root.render(<AccountManagement language={language} currentAccountId={currentAccountId} onAccountChanged={onAccountChanged} onBusyChange={onBusyChange} />)
    await settle()
  })
}
async function rerender(accountId: string, language: 'en' | 'ja' = 'en') {
  currentAccountId = accountId
  await act(async () => {
    root.render(<AccountManagement language={language} currentAccountId={currentAccountId} onAccountChanged={onAccountChanged} onBusyChange={onBusyChange} />)
    await settle()
  })
}
function setInput(input: HTMLInputElement, value: string) {
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
}
function button(testId: string) {
  const found = host.querySelector<HTMLButtonElement>(`[data-testid="${testId}"]`)
  if (!found) throw new Error(`Button missing: ${testId}`)
  return found
}
function submit(selector: string) {
  const form = host.querySelector<HTMLFormElement>(selector)
  if (!form) throw new Error(`Form missing: ${selector}`)
  form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
}
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}

beforeEach(() => {
  vi.resetAllMocks()
  window.localStorage.clear()
  currentAccountId = admin.id
  onAccountChanged = vi.fn(async () => undefined)
  onBusyChange = vi.fn((_busy: boolean) => undefined)
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
  vi.mocked(guiRead).mockImplementation(async (operation) => {
    if (operation === 'management.capabilities') return fullCapabilities as never
    if (operation === 'account.list') return { items: [admin, member, disabledMember] } as never
    if (operation === 'account.get') return member as never
    throw new Error(`Unexpected read operation: ${operation}`)
  })
  vi.mocked(guiCreationStatus).mockResolvedValue(readyCreation)
  vi.mocked(guiAccountManage).mockImplementation(async (operation, args) => {
    if (operation === 'account.create') {
      const createArgs = args as { name: string; password: string }
      return { id: `created-${createArgs.name}`, name: createArgs.name, is_admin: false, disabled: false, role_ids: [] } as never
    }
    const mutationArgs = args as { id: string; disabled?: boolean; password?: string }
    const id = mutationArgs.id
    const original = [admin, member, disabledMember].find((account) => account.id === id) ?? { ...member, id, name: 'New account' }
    if (operation === 'account.disable') return { ...original, disabled: mutationArgs.disabled } as never
    return original as never
  })
})

afterEach(async () => {
  if (root) await act(async () => root.unmount())
  host?.remove()
})

describe('account management permissions and mutations', () => {
  it('hides write controls for a member without management grants', async () => {
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'management.capabilities') return noCapabilities as never
      if (operation === 'account.list') return { items: [admin, member] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()

    expect(host.querySelector('[data-testid="account-management"]')).not.toBeNull()
    expect(host.textContent).toContain('not available to this account')
    expect(host.querySelector('[data-testid="account-list"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-create-toggle"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-disable-member-1"]')).toBeNull()
    expect(guiAccountManage).not.toHaveBeenCalled()
  })

  it('shows delegated scoped operations but never exposes controls for administrator accounts', async () => {
    const scoped: ManagementCapabilities = { ...fullCapabilities, account_create: false }
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'management.capabilities') return scoped as never
      if (operation === 'account.list') return { items: [admin, member] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()

    expect(host.querySelector('[data-testid="account-create-toggle"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-disable-admin-1"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-reset-admin-1"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-disable-member-1"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="account-reset-member-1"]')).not.toBeNull()
  })

  it('keeps a list-only grant usable without exposing account mutation controls', async () => {
    const listOnly: ManagementCapabilities = { ...noCapabilities, account_list: true }
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'management.capabilities') return listOnly as never
      if (operation === 'account.list') return { items: [admin, member] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()

    expect(host.querySelector('[data-testid="account-list"]')?.textContent).toContain('Rin')
    expect(host.querySelector('[data-testid="account-management-refresh"]')).not.toBeNull()
    expect(host.textContent).not.toContain('not available to this account')
    expect(host.querySelector('[data-testid="account-create-toggle"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-disable-member-1"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-reset-member-1"]')).toBeNull()
  })

  it('clears creation passwords immediately, serializes duplicate submissions, and never persists secrets', async () => {
    const operation = deferred<Account>()
    vi.mocked(guiAccountManage).mockReturnValue(operation.promise)
    await render()
    await act(async () => { button('account-create-toggle').click(); await settle() })
    const name = host.querySelector<HTMLInputElement>('[data-testid="account-create-name"]')!
    const password = host.querySelector<HTMLInputElement>('[data-testid="account-create-password"]')!
    setInput(name, 'Temporary operator')
    setInput(password, 'one-time-secret')
    await act(async () => {
      submit('[data-testid="account-create-form"]')
      submit('[data-testid="account-create-form"]')
      await settle()
    })

    expect(host.querySelector('[data-testid="account-create-password"]')).toBeNull()
    expect(guiAccountManage).toHaveBeenCalledTimes(1)
    expect(guiAccountManage).toHaveBeenCalledWith('account.create', { name: 'Temporary operator', password: 'one-time-secret' })
    expect(onBusyChange).toHaveBeenCalledWith(true)
    expect(host.textContent).not.toContain('one-time-secret')
    expect(Array.from({ length: window.localStorage.length }, (_, index) => window.localStorage.key(index)).join(' ')).not.toContain('one-time-secret')
    expect(vi.mocked(guiRead).mock.calls.some(([, args]) => (JSON.stringify(args) ?? '').includes('one-time-secret'))).toBe(false)

    await act(async () => {
      operation.resolve({ id: 'new-account', name: 'Temporary operator', is_admin: false, disabled: false, role_ids: [] })
      await operation.promise
      await settle()
    })
    expect(host.querySelector('[data-testid="account-mutation-notice"]')?.textContent).toContain('Account created')
    expect(onBusyChange).toHaveBeenLastCalledWith(false)
    await act(async () => { button('account-create-toggle').click(); await settle() })
    expect(host.querySelector<HTMLInputElement>('[data-testid="account-create-password"]')?.value).toBe('')
  })

  it('clears a reset password immediately and sends it only through the reset mutation', async () => {
    const operation = deferred<Account>()
    vi.mocked(guiAccountManage).mockReturnValue(operation.promise)
    await render()
    await act(async () => { button('account-reset-member-1').click(); await settle() })
    const password = host.querySelector<HTMLInputElement>('[data-testid="account-reset-password"]')!
    setInput(password, 'replacement-secret')
    await act(async () => { submit('[data-testid="account-reset-form"]'); await settle() })

    expect(host.querySelector('[data-testid="account-reset-password"]')).toBeNull()
    expect(guiAccountManage).toHaveBeenCalledWith('account.reset', { id: member.id, password: 'replacement-secret' })
    expect(host.textContent).not.toContain('replacement-secret')
    expect(Array.from({ length: window.localStorage.length }, (_, index) => window.localStorage.key(index)).join(' ')).not.toContain('replacement-secret')
    expect(vi.mocked(guiRead).mock.calls.some(([, args]) => (JSON.stringify(args) ?? '').includes('replacement-secret'))).toBe(false)

    await act(async () => {
      operation.resolve(member)
      await operation.promise
      await settle()
    })
    expect(host.querySelector('[data-testid="account-mutation-notice"]')?.textContent).toContain('Account updated')
    await act(async () => { button('account-reset-member-1').click(); await settle() })
    expect(host.querySelector<HTMLInputElement>('[data-testid="account-reset-password"]')?.value).toBe('')
  })

  it('requires explicit disable confirmation and refreshes the current session when disabling itself', async () => {
    currentAccountId = member.id
    let accounts = [admin, member, disabledMember]
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'management.capabilities') return fullCapabilities as never
      if (operation === 'account.list') return { items: accounts } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    vi.mocked(guiAccountManage).mockImplementation(async (operation, args) => {
      expect(operation).toBe('account.disable')
      expect(args).toEqual({ id: member.id, disabled: true })
      const updated = { ...member, disabled: true }
      accounts = [admin, updated, disabledMember]
      return updated
    })
    await render()
    await act(async () => { button('account-disable-member-1').click(); await settle() })
    expect(guiAccountManage).not.toHaveBeenCalled()
    expect(host.querySelector('[role="group"][aria-label="Disable account"]')?.textContent).toContain('revokes its current logins')
    await act(async () => { button('account-confirm-action').click(); await settle() })

    expect(guiAccountManage).toHaveBeenCalledTimes(1)
    expect(onAccountChanged).toHaveBeenCalledTimes(1)
    expect(host.querySelector('[data-testid="account-row-member-1"]')?.textContent).toContain('Disabled')
    expect(host.querySelector('[data-testid="account-mutation-notice"]')?.textContent).toContain('Account updated')
  })

  it('fails closed while a durable creation request is pending', async () => {
    vi.mocked(guiCreationStatus).mockResolvedValue({ pending: true, operation: 'post.create', expired: false })
    await render()
    expect(host.querySelector('[data-testid="account-creation-pending"]')?.textContent).toContain('creation request is pending')
    expect(button('account-create-toggle').disabled).toBe(true)
    expect(button('account-disable-member-1').disabled).toBe(true)
    expect(button('account-reset-member-1').disabled).toBe(true)
    expect(guiAccountManage).not.toHaveBeenCalled()
  })

  it('retries failed capabilities and account-list reads before exposing controls', async () => {
    let capabilityFails = true
    let listFails = true
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'management.capabilities') {
        if (capabilityFails) { capabilityFails = false; throw new Error('capability read failed') }
        return fullCapabilities as never
      }
      if (operation === 'account.list') {
        if (listFails) { listFails = false; throw new Error('account list failed') }
        return { items: [admin, member] } as never
      }
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    expect(host.querySelector('[data-testid="account-management-error"]')).not.toBeNull()
    await act(async () => { button('account-management-retry').click(); await settle() })
    expect(host.querySelector('[data-testid="account-management-error"]')).not.toBeNull()
    expect(host.querySelector<HTMLButtonElement>('[data-testid="account-create-toggle"]')?.disabled).toBe(true)
    await act(async () => { button('account-management-retry').click(); await settle() })
    expect(host.querySelector('[data-testid="account-management-error"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-disable-member-1"]')).not.toBeNull()
  })

  it('gates each account operation by refreshed capabilities', async () => {
    let capabilities = fullCapabilities
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'management.capabilities') return capabilities as never
      if (operation === 'account.list') return { items: [admin, member] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    expect(host.querySelector('[data-testid="account-reset-member-1"]')).not.toBeNull()
    await act(async () => { button('account-reset-member-1').click(); await settle() })
    setInput(host.querySelector<HTMLInputElement>('[data-testid="account-reset-password"]')!, 'temporary-secret')
    expect(host.querySelector('[data-testid="account-reset-form"]')).not.toBeNull()
    capabilities = noWritesCapabilities
    await act(async () => { button('account-management-refresh').click(); await settle() })
    expect(host.querySelector('[data-testid="account-create-toggle"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-disable-member-1"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-reset-member-1"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-reset-form"]')).toBeNull()
    expect(host.querySelector('[data-testid="account-reset-password"]')).toBeNull()
    expect(host.textContent).not.toContain('temporary-secret')
  })

  it('requires a fresh read after an uncertain mutation instead of retrying it', async () => {
    vi.mocked(guiAccountManage).mockRejectedValueOnce(new Error('response lost after mutation'))
    await render()
    await act(async () => { button('account-create-toggle').click(); await settle() })
    setInput(host.querySelector<HTMLInputElement>('[data-testid="account-create-name"]')!, 'One account')
    setInput(host.querySelector<HTMLInputElement>('[data-testid="account-create-password"]')!, 'secret')
    await act(async () => { submit('[data-testid="account-create-form"]'); await settle() })
    expect(host.querySelector('[data-testid="account-mutation-error"]')?.textContent).toContain('could not be confirmed')
    expect(button('account-create-toggle').disabled).toBe(true)
    expect(guiAccountManage).toHaveBeenCalledTimes(1)

    await act(async () => { button('account-management-refresh').click(); await settle() })
    expect(host.querySelector('[data-testid="account-mutation-error"]')).not.toBeNull()
    expect(button('account-create-toggle').disabled).toBe(false)
    expect(guiAccountManage).toHaveBeenCalledTimes(1)
  })

  it('does not leak a delayed account-list response across an account switch', async () => {
    let resolveAccountA!: (result: { items: Account[] }) => void
    let listCall = 0
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'management.capabilities') return fullCapabilities as never
      if (operation === 'account.list') {
        listCall += 1
        if (listCall === 1) return new Promise((resolve) => { resolveAccountA = resolve }) as never
        return { items: [{ ...member, id: 'account-b-member', name: 'Account B private name' }] } as never
      }
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    expect(host.querySelector('[data-testid="account-management-loading"]')).not.toBeNull()
    await rerender('account-b')
    expect(host.textContent).toContain('Account B private name')
    await act(async () => { resolveAccountA({ items: [{ ...member, id: 'account-a-member', name: 'Stale Account A name' }] }); await settle() })
    expect(host.textContent).toContain('Account B private name')
    expect(host.textContent).not.toContain('Stale Account A name')
  })

  it('ignores delayed account data after unmount', async () => {
    let resolveList!: (result: { items: Account[] }) => void
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'management.capabilities') return fullCapabilities as never
      if (operation === 'account.list') return new Promise((resolve) => { resolveList = resolve }) as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => root.unmount())
    await act(async () => { resolveList({ items: [{ ...member, name: 'Unmounted secret row' }] }); await settle() })
    expect(host.textContent).not.toContain('Unmounted secret row')
  })
})
