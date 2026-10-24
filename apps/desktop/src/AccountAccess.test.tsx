// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'
import AccountAccess from './AccountAccess'
import { authenticate, getSnapshot, guiCreationStatus, revealRecoveryCode } from './bridge'
import type { Account, CreationStatus, GuiSnapshot } from './types'

vi.mock('./bridge', () => ({
  authenticate: vi.fn(),
  getSnapshot: vi.fn(),
  guiCreationStatus: vi.fn(),
  revealRecoveryCode: vi.fn(),
}))

const admin: Account = { id: 'admin-a', name: 'Ada', is_admin: true, disabled: false, role_ids: [] }
const adminB: Account = { id: 'admin-b', name: 'Bea', is_admin: true, disabled: false, role_ids: [] }
const member: Account = { id: 'member-a', name: 'Rin', is_admin: false, disabled: false, role_ids: ['reader'] }
const noPendingCreation: CreationStatus = { pending: false, operation: null, expired: false }
const pendingCreation: CreationStatus = { pending: true, operation: 'post.create', expired: false }
const signedInAsMember: GuiSnapshot = {
  setupRequired: false,
  account: member,
  pendingIssue: false,
}

let host: HTMLDivElement
let root: Root
let activeAccount = admin
let onAccountChanged: Mock<(snapshot: GuiSnapshot) => void>
let onUncertain: Mock<() => void>
let beginOperation: Mock<() => boolean>
let endOperation: Mock<() => void>

async function settle() { await new Promise<void>((resolve) => setTimeout(resolve, 0)) }
async function render(language: 'en' | 'ja' = 'en') {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  await act(async () => {
    root.render(<AccountAccess language={language} account={activeAccount} onAccountChanged={onAccountChanged} onUncertain={onUncertain} beginOperation={beginOperation} endOperation={endOperation} />)
    await settle()
  })
}
async function rerender(account: Account, language: 'en' | 'ja' = 'en') {
  activeAccount = account
  await act(async () => {
    root.render(<AccountAccess language={language} account={activeAccount} onAccountChanged={onAccountChanged} onUncertain={onUncertain} beginOperation={beginOperation} endOperation={endOperation} />)
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
function submit() {
  const form = host.querySelector<HTMLFormElement>('[data-testid="account-switch-form"]')
  if (!form) throw new Error('Account switch form is not rendered')
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
  activeAccount = admin
  onAccountChanged = vi.fn((_snapshot: GuiSnapshot) => undefined)
  onUncertain = vi.fn(() => undefined)
  beginOperation = vi.fn(() => true)
  endOperation = vi.fn(() => undefined)
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
  vi.mocked(guiCreationStatus).mockResolvedValue(noPendingCreation)
  vi.mocked(authenticate).mockResolvedValue(signedInAsMember)
  vi.mocked(getSnapshot).mockResolvedValue({ setupRequired: false, account: admin, pendingIssue: false })
  vi.mocked(revealRecoveryCode).mockResolvedValue({ recovery_code: 'R-4COVERY-ONLY' })
})

afterEach(async () => {
  if (root) await act(async () => root.unmount())
  host?.remove()
})

describe('account access controls', () => {
  it('keeps the current account active through failed manual sign-in and clears the password', async () => {
    vi.mocked(authenticate).mockRejectedValueOnce(new Error('wrong password'))
    await render()
    await act(async () => { button('account-switch-toggle').click(); await settle() })
    setInput(host.querySelector<HTMLInputElement>('[data-testid="account-switch-name"]')!, 'Rin')
    const password = host.querySelector<HTMLInputElement>('[data-testid="account-switch-password"]')!
    setInput(password, 'not-the-password')
    await act(async () => { submit(); await settle() })

    expect(password.value).toBe('')
    expect(authenticate).toHaveBeenCalledWith('login', 'Rin', 'not-the-password')
    expect(onAccountChanged).toHaveBeenCalledWith({ setupRequired: false, account: admin, pendingIssue: false })
    expect(host.querySelector('[data-testid="account-access"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="account-switch-error"]')?.textContent).toContain('Could not switch accounts')
    expect(host.textContent).not.toContain('not-the-password')
    expect(Array.from({ length: window.localStorage.length }, (_, index) => window.localStorage.key(index)).join(' ')).not.toContain('not-the-password')
  })

  it('refuses account switching while creation is pending and fails closed when its status is unavailable', async () => {
    await render()
    await act(async () => { button('account-switch-toggle').click(); await settle() })
    setInput(host.querySelector<HTMLInputElement>('[data-testid="account-switch-name"]')!, 'Rin')
    const password = host.querySelector<HTMLInputElement>('[data-testid="account-switch-password"]')!
    setInput(password, 'secret')
    vi.mocked(guiCreationStatus).mockResolvedValueOnce(pendingCreation)
    await act(async () => { submit(); await settle() })
    expect(password.value).toBe('')
    expect(authenticate).not.toHaveBeenCalled()
    expect(onAccountChanged).not.toHaveBeenCalled()
    expect(host.querySelector('[data-testid="account-switch-error"]')?.textContent).toContain('creation request is pending')

    vi.mocked(guiCreationStatus).mockRejectedValueOnce(new Error('status read failed'))
    setInput(password, 'secret-again')
    await act(async () => { submit(); await settle() })
    expect(password.value).toBe('')
    expect(authenticate).not.toHaveBeenCalled()
    expect(host.querySelector('[data-testid="account-switch-error"]')?.textContent).toContain('Could not verify pending creation')
  })

  it('serializes switch attempts and changes account only after pending status and authentication succeed', async () => {
    const status = deferred<CreationStatus>()
    const snapshot = deferred<GuiSnapshot>()
    vi.mocked(guiCreationStatus).mockReturnValue(status.promise)
    vi.mocked(authenticate).mockReturnValue(snapshot.promise)
    await render()
    await act(async () => { button('account-switch-toggle').click(); await settle() })
    setInput(host.querySelector<HTMLInputElement>('[data-testid="account-switch-name"]')!, 'Rin')
    const password = host.querySelector<HTMLInputElement>('[data-testid="account-switch-password"]')!
    setInput(password, 'valid-secret')
    await act(async () => { submit(); submit(); await settle() })

    expect(password.value).toBe('')
    expect(guiCreationStatus).toHaveBeenCalledTimes(1)
    expect(authenticate).not.toHaveBeenCalled()
    expect(onAccountChanged).not.toHaveBeenCalled()
    await act(async () => { status.resolve(noPendingCreation); await status.promise; await settle() })
    expect(authenticate).toHaveBeenCalledTimes(1)
    expect(onAccountChanged).not.toHaveBeenCalled()
    await act(async () => { snapshot.resolve(signedInAsMember); await snapshot.promise; await settle() })
    expect(onAccountChanged).toHaveBeenCalledWith(signedInAsMember)
    expect(beginOperation).toHaveBeenCalledTimes(1)
    expect(endOperation).toHaveBeenCalledTimes(1)
  })

  it('does not automatically reveal a recovery code and allows explicit hide then reveal', async () => {
    await render()
    expect(host.querySelector('[data-testid="account-switch-toggle"]')?.textContent).toContain('Switch account')
    expect(host.querySelector('[data-testid="recovery-code-value"]')).toBeNull()
    expect(revealRecoveryCode).not.toHaveBeenCalled()

    await act(async () => { button('recovery-code-toggle').click(); await settle() })
    expect(revealRecoveryCode).toHaveBeenCalledTimes(1)
    expect(host.querySelector('[data-testid="recovery-code-value"]')?.textContent).toContain('R-4COVERY-ONLY')
    await act(async () => { button('recovery-code-hide').click(); await settle() })
    expect(host.querySelector('[data-testid="recovery-code-value"]')).toBeNull()
    expect(host.textContent).not.toContain('R-4COVERY-ONLY')

    await act(async () => { button('recovery-code-toggle').click(); await settle() })
    expect(revealRecoveryCode).toHaveBeenCalledTimes(2)
  })

  it('only renders the recovery-code action for an administrator', async () => {
    activeAccount = member
    await render()
    expect(host.querySelector('[data-testid="recovery-code-toggle"]')).toBeNull()
    expect(revealRecoveryCode).not.toHaveBeenCalled()
  })

  it('clears a delayed revealed code when the current account changes', async () => {
    const reveal = deferred<{ recovery_code: string }>()
    vi.mocked(revealRecoveryCode).mockReturnValue(reveal.promise)
    await render()
    await act(async () => { button('recovery-code-toggle').click(); await settle() })
    expect(beginOperation).toHaveBeenCalledTimes(1)
    await rerender(adminB)
    await act(async () => { reveal.resolve({ recovery_code: 'STALE-RECOVERY-CODE' }); await reveal.promise; await settle() })

    expect(host.querySelector('[data-testid="recovery-code-value"]')).toBeNull()
    expect(host.textContent).not.toContain('STALE-RECOVERY-CODE')

    vi.mocked(revealRecoveryCode).mockResolvedValueOnce({ recovery_code: 'VISIBLE-THEN-REVOKED' })
    await act(async () => { button('recovery-code-toggle').click(); await settle() })
    expect(host.querySelector('[data-testid="recovery-code-value"]')?.textContent).toContain('VISIBLE-THEN-REVOKED')
    await rerender(member)
    expect(host.querySelector('[data-testid="recovery-code-value"]')).toBeNull()
    expect(host.textContent).not.toContain('VISIBLE-THEN-REVOKED')
  })

  it('does not retain an explicitly revealed code after the account-access section remounts', async () => {
    await render('ja')
    expect(host.querySelector('[data-testid="recovery-code-toggle"]')?.textContent).toContain('復旧コードを表示')
    await act(async () => { button('recovery-code-toggle').click(); await settle() })
    expect(host.querySelector('[data-testid="recovery-code-value"]')?.textContent).toContain('R-4COVERY-ONLY')
    await act(async () => root.unmount())
    host.replaceChildren()
    root = createRoot(host)
    await act(async () => {
      root.render(<AccountAccess language="ja" account={admin} onAccountChanged={onAccountChanged} onUncertain={onUncertain} beginOperation={beginOperation} endOperation={endOperation} />)
      await settle()
    })
    expect(host.querySelector('[data-testid="recovery-code-value"]')).toBeNull()
    expect(host.querySelector('[data-testid="recovery-code-toggle"]')?.textContent).toContain('復旧コードを表示')
  })
})
