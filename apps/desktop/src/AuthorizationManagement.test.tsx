// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'
import AuthorizationManagement from './AuthorizationManagement'
import { guiAuthorizationManage, guiCreationStatus, guiRead } from './bridge'
import type { Account, AuthorizationCapabilities, CreationStatus, ManagementCapabilities, PermissionSetArgs, Role } from './types'

vi.mock('./bridge', () => ({
  guiAuthorizationManage: vi.fn(),
  guiCreationStatus: vi.fn(),
  guiRead: vi.fn(),
}))

const admin: Account = { id: 'admin-1', name: 'Primary admin', is_admin: true, disabled: false, role_ids: ['role-reviewer'] }
const member: Account = { id: 'member-1', name: 'Rin', is_admin: false, disabled: false, role_ids: ['role-reviewer'] }
const roles: Role[] = [
  { id: 'role-reviewer', name: 'Reviewer', created_at: '2026-10-01T00:00:00Z' },
  { id: 'role-editor', name: 'Editor', created_at: '2026-10-02T00:00:00Z' },
]
const fullAuthorization: AuthorizationCapabilities = {
  role_create: true,
  role_list: true,
  role_assign: true,
  permission_set: true,
  permission_list: true,
  operations: [
    { operation: 'board.create', scopes: ['global'] },
    { operation: 'thread.create', scopes: ['board', 'thread'] },
    { operation: 'post.create', scopes: ['thread'] },
  ],
}
const fullManagement: ManagementCapabilities = {
  account_list: true, account_get: true, account_create: true, account_disable: true, account_reset: true,
}
const noPending: CreationStatus = { pending: false, operation: null, expired: false }
const pending: CreationStatus = { pending: true, operation: 'post.create', expired: false }

let host: HTMLDivElement
let root: Root
let activeAccountId = admin.id
let serverAccounts: Account[]
let serverRoles: Role[]
let serverRules: PermissionSetArgs[]
let authorization: AuthorizationCapabilities
let management: ManagementCapabilities
let creationStatus: CreationStatus
let beginOperation: Mock<() => boolean>
let endOperation: Mock<() => void>
let onAuthorizationChanged: Mock<() => Promise<void>>

async function settle() { await new Promise<void>((resolve) => setTimeout(resolve, 0)) }
async function render(language: 'en' | 'ja' = 'en') {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  await act(async () => {
    root.render(<AuthorizationManagement language={language} currentAccountId={activeAccountId} beginOperation={() => beginOperation()} endOperation={() => endOperation()} onAuthorizationChanged={() => onAuthorizationChanged()} />)
    await settle()
  })
}
async function rerender(accountId: string, language: 'en' | 'ja' = 'en') {
  activeAccountId = accountId
  await act(async () => {
    root.render(<AuthorizationManagement language={language} currentAccountId={activeAccountId} beginOperation={() => beginOperation()} endOperation={() => endOperation()} onAuthorizationChanged={() => onAuthorizationChanged()} />)
    await settle()
  })
}
function button(testId: string) {
  const found = host.querySelector<HTMLButtonElement>(`[data-testid="${testId}"]`)
  if (!found) throw new Error(`Button missing: ${testId}`)
  return found
}
function setInput(testId: string, value: string) {
  const input = host.querySelector<HTMLInputElement>(`[data-testid="${testId}"]`)
  if (!input) throw new Error(`Input missing: ${testId}`)
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
}
function choosePermission(operation: string, decision: PermissionSetArgs['decision']) {
  const input = host.querySelector<HTMLInputElement>(`[data-testid="authorization-decision-${operation}-${decision}"]`)
  if (!input) throw new Error(`Permission toggle missing: ${operation}/${decision}`)
  act(() => input.click())
}
function submit(testId: string) {
  const form = host.querySelector<HTMLFormElement>(`[data-testid="${testId}"]`)
  if (!form) throw new Error(`Form missing: ${testId}`)
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
  activeAccountId = admin.id
  serverAccounts = [admin, member]
  serverRoles = [...roles]
  serverRules = []
  authorization = fullAuthorization
  management = fullManagement
  creationStatus = noPending
  beginOperation = vi.fn(() => true)
  endOperation = vi.fn(() => undefined)
  onAuthorizationChanged = vi.fn(async () => undefined)
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
  vi.mocked(guiRead).mockImplementation(async (operation) => {
    if (operation === 'authorization.capabilities') return authorization as never
    if (operation === 'management.capabilities') return management as never
    if (operation === 'account.list') return { items: serverAccounts } as never
    if (operation === 'role.list') return { items: serverRoles } as never
    if (operation === 'permission.list') return { items: serverRules } as never
    throw new Error(`Unexpected read operation: ${operation}`)
  })
  vi.mocked(guiCreationStatus).mockImplementation(async () => creationStatus)
  vi.mocked(guiAuthorizationManage).mockImplementation(async (operation, args) => {
    if (operation === 'role.create') {
      const createArgs = args as { name: string }
      const created = { id: `role-${createArgs.name.toLowerCase().replace(/\s+/g, '-')}`, name: createArgs.name }
      serverRoles = [...serverRoles, created]
      return created as never
    }
    if (operation === 'role.assign') {
      const assignmentArgs = args as { account_id: string; role_id: string; assigned: boolean }
      serverAccounts = serverAccounts.map((account) => account.id !== assignmentArgs.account_id ? account : {
        ...account,
        role_ids: assignmentArgs.assigned ? [...new Set([...account.role_ids, assignmentArgs.role_id])] : account.role_ids.filter((id) => id !== assignmentArgs.role_id),
      })
      return args as never
    }
    const permissionArgs = args as PermissionSetArgs
    serverRules = permissionArgs.decision === 'inherit'
      ? serverRules.filter((rule) => !(rule.operation === permissionArgs.operation && rule.scope === permissionArgs.scope && rule.target_id === permissionArgs.target_id && rule.subject_type === permissionArgs.subject_type && rule.subject_id === permissionArgs.subject_id))
      : [...serverRules.filter((rule) => !(rule.operation === permissionArgs.operation && rule.scope === permissionArgs.scope && rule.target_id === permissionArgs.target_id && rule.subject_type === permissionArgs.subject_type && rule.subject_id === permissionArgs.subject_id)), permissionArgs]
    return args as never
  })
})

afterEach(async () => {
  if (root) await act(async () => root.unmount())
  host?.remove()
})

describe('authorization management', () => {
  it('shows mutually exclusive permission states and drops a draft reverted to its saved value', async () => {
    await render()
    expect(host.querySelector('[data-testid="authorization-operation"]')).toBeNull()
    expect(host.querySelector('[data-testid="authorization-decision"]')).toBeNull()
    const checked = () => [...host.querySelectorAll<HTMLInputElement>('[data-testid="authorization-permission-row-board.create"] input:checked')].map((input) => input.dataset.testid)
    expect(checked()).toEqual(['authorization-decision-board.create-inherit'])
    choosePermission('board.create', 'allow')
    expect(checked()).toEqual(['authorization-decision-board.create-allow'])
    expect(button('authorization-permission-review').textContent).toContain('(1)')
    choosePermission('board.create', 'deny')
    expect(checked()).toEqual(['authorization-decision-board.create-deny'])
    choosePermission('board.create', 'inherit')
    expect(checked()).toEqual(['authorization-decision-board.create-inherit'])
    expect(button('authorization-permission-review').disabled).toBe(true)
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
  })

  it('reviews multiple row changes together and saves only after confirmation', async () => {
    authorization = { ...fullAuthorization, operations: [...fullAuthorization.operations, { operation: 'post.edit', scopes: ['global'] }] }
    await render()
    choosePermission('board.create', 'allow')
    choosePermission('post.edit', 'deny')
    expect(button('authorization-permission-review').textContent).toContain('(2)')
    await act(async () => { submit('authorization-permission-form'); await settle() })
    expect(host.querySelector('[data-testid="authorization-change-summary"]')?.textContent).toContain('board.create')
    expect(host.querySelector('[data-testid="authorization-change-summary"]')?.textContent).toContain('post.edit')
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
    await act(async () => { button('authorization-cancel').click(); await settle() })
    expect(button('authorization-permission-review').textContent).toContain('(2)')
    await act(async () => { submit('authorization-permission-form'); await settle() })
    await act(async () => { button('authorization-confirm').click(); await settle() })
    expect(guiAuthorizationManage).toHaveBeenCalledTimes(2)
    expect(guiAuthorizationManage).toHaveBeenNthCalledWith(1, 'permission.set', expect.objectContaining({ operation: 'board.create', decision: 'allow' }))
    expect(guiAuthorizationManage).toHaveBeenNthCalledWith(2, 'permission.set', expect.objectContaining({ operation: 'post.edit', decision: 'deny' }))
    expect(onAuthorizationChanged).toHaveBeenCalledTimes(1)
    expect(button('authorization-permission-review').disabled).toBe(true)
  })

  it('clears unsaved rows and confirmations when switching subject or scope', async () => {
    await render()
    choosePermission('board.create', 'deny')
    await act(async () => { submit('authorization-permission-form'); await settle() })
    await act(async () => {
      const select = host.querySelector<HTMLSelectElement>('[data-testid="authorization-subject-type"]')!
      select.value = 'role'
      select.dispatchEvent(new Event('change', { bubbles: true }))
      await settle()
    })
    expect(host.querySelector('[data-testid="authorization-confirmation"]')).toBeNull()
    expect(host.querySelector<HTMLSelectElement>('[data-testid="authorization-subject"]')?.value).toBe('role-reviewer')
    expect(button('authorization-permission-review').disabled).toBe(true)
    choosePermission('board.create', 'allow')
    await act(async () => { button('authorization-scope-thread').click(); await settle() })
    expect(button('authorization-permission-review').disabled).toBe(true)
    expect(host.querySelector('[data-testid="authorization-permission-row-board.create"]')).toBeNull()
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
  })

  it('does not show a saved inheritance state without permission-list authority', async () => {
    authorization = { ...fullAuthorization, permission_list: false }
    await render()
    expect(host.querySelectorAll('[data-testid="authorization-permission-matrix"] input:checked')).toHaveLength(0)
    expect(host.textContent).toContain('cannot read existing rules')
    choosePermission('board.create', 'inherit')
    await act(async () => { submit('authorization-permission-form'); await settle() })
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
    expect(host.querySelector('[data-testid="authorization-change-summary"]')?.textContent).toContain('Inherit')
  })

  it('stops a partially saved batch and requires readback before another change', async () => {
    authorization = { ...fullAuthorization, operations: [...fullAuthorization.operations, { operation: 'post.edit', scopes: ['global'] }, { operation: 'read', scopes: ['global'] }] }
    const save = vi.mocked(guiAuthorizationManage).getMockImplementation()!
    vi.mocked(guiAuthorizationManage).mockImplementationOnce(save).mockRejectedValueOnce(new Error('second response lost'))
    await render()
    choosePermission('board.create', 'allow')
    choosePermission('post.edit', 'deny')
    choosePermission('read', 'deny')
    await act(async () => { submit('authorization-permission-form'); await settle() })
    await act(async () => { button('authorization-confirm').click(); await settle() })
    expect(guiAuthorizationManage).toHaveBeenCalledTimes(2)
    expect(host.querySelector('[data-testid="authorization-mutation-error"]')?.textContent).toContain('result could not be confirmed')
    expect(button('authorization-permission-review').disabled).toBe(true)
    expect(onAuthorizationChanged).toHaveBeenCalledTimes(1)
    await act(async () => { button('authorization-refresh').click(); await settle() })
    expect(host.querySelector<HTMLInputElement>('[data-testid="authorization-decision-board.create-allow"]')?.checked).toBe(true)
    expect(host.querySelector<HTMLInputElement>('[data-testid="authorization-decision-post.edit-inherit"]')?.checked).toBe(true)
    expect(host.querySelector<HTMLInputElement>('[data-testid="authorization-decision-read-inherit"]')?.checked).toBe(true)
    expect(button('authorization-permission-review').disabled).toBe(true)
    expect(guiAuthorizationManage).toHaveBeenCalledTimes(2)
  })

  it('fails closed without authorization grants and supports a read-only role list', async () => {
    authorization = { ...fullAuthorization, role_create: false, role_assign: false, permission_set: false }
    await render()
    expect(host.querySelector('[data-testid="authorization-management"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="authorization-role-list"]')?.textContent).toContain('Reviewer')
    expect(host.querySelector('[data-testid="authorization-role-create-toggle"]')).toBeNull()
    expect(host.querySelector('[data-testid="authorization-permission-form"]')).toBeNull()
    expect(host.querySelector('[data-testid="authorization-account-row-member-1"]')).toBeNull()
    expect(host.querySelector('[data-testid="authorization-refresh"]')).not.toBeNull()
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
  })

  it('excludes the main administrator from assignment and permission subjects', async () => {
    await render()
    expect(host.querySelector('[data-testid="authorization-account-row-admin-1"]')).toBeNull()
    expect(host.querySelector('[data-testid="authorization-account-row-member-1"]')).not.toBeNull()
    const subjectOptions = [...host.querySelectorAll<HTMLOptionElement>('[data-testid="authorization-subject"] option')].map((option) => option.textContent)
    expect(subjectOptions.join(' ')).toContain('Rin')
    expect(subjectOptions.join(' ')).not.toContain('Primary admin')
    expect(host.querySelector('[data-testid="authorization-role-assignment-admin-1-role-reviewer"]')).toBeNull()
  })

  it('requires confirmation before creating a role and clears the form when canceled', async () => {
    await render()
    await act(async () => { button('authorization-role-create-toggle').click(); await settle() })
    setInput('authorization-role-name', 'Triage helpers')
    await act(async () => { submit('authorization-role-create-form'); await settle() })
    expect(host.querySelector('[data-testid="authorization-change-summary"]')?.textContent).toContain('Triage helpers')
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
    await act(async () => { button('authorization-cancel').click(); await settle() })
    expect(host.querySelector('[data-testid="authorization-confirmation"]')).toBeNull()
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
    expect(host.querySelector<HTMLInputElement>('[data-testid="authorization-role-name"]')?.value).toBe('Triage helpers')

    await act(async () => { submit('authorization-role-create-form'); await settle() })
    await act(async () => { button('authorization-confirm').click(); await settle() })
    expect(guiAuthorizationManage).toHaveBeenCalledWith('role.create', { name: 'Triage helpers' })
    expect(onAuthorizationChanged).toHaveBeenCalledTimes(1)
    expect(host.querySelector('[data-testid="authorization-role-list"]')?.textContent).toContain('Triage helpers')
  })

  it('confirms role assignment and unassignment without allowing admin membership changes', async () => {
    await render()
    const assignment = 'authorization-role-assignment-member-1-role-reviewer'
    expect(button(assignment).textContent).toBe('Unassign')
    await act(async () => { button(assignment).click(); await settle() })
    expect(host.querySelector('[data-testid="authorization-change-summary"]')?.textContent).toContain('Unassign')
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
    await act(async () => { button('authorization-confirm').click(); await settle() })
    expect(guiAuthorizationManage).toHaveBeenCalledWith('role.assign', { account_id: member.id, role_id: roles[0].id, assigned: false })
    expect(button(assignment).textContent).toBe('Assign')
    expect(onAuthorizationChanged).toHaveBeenCalledTimes(1)
  })

  it('allows only catalog-supported scopes and clears a target when switching back to global scope', async () => {
    await render()
    await act(async () => { button('authorization-scope-board').click(); await settle() })
    expect(host.querySelector('[data-testid="authorization-permission-row-board.create"]')).toBeNull()
    expect(host.querySelector('[data-testid="authorization-permission-row-thread.create"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="authorization-permission-row-post.create"]')).toBeNull()
    expect(host.querySelector('[data-testid="authorization-target-id"]')).not.toBeNull()
    setInput('authorization-target-id', 'board-private')
    await act(async () => { button('authorization-scope-global').click(); await settle() })
    expect(button('authorization-scope-global').getAttribute('aria-pressed')).toBe('true')
    expect(host.querySelector('[data-testid="authorization-target-id"]')).toBeNull()
    choosePermission('board.create', 'allow')
    await act(async () => { submit('authorization-permission-form'); await settle() })
    expect(host.querySelector('[data-testid="authorization-confirmation"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="authorization-change-summary"]')?.textContent).toContain('Global')
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
    await act(async () => { button('authorization-confirm').click(); await settle() })
    expect(guiAuthorizationManage).toHaveBeenCalledWith('permission.set', { subject_type: 'account', subject_id: member.id, operation: 'board.create', scope: 'global', target_id: '', decision: 'allow' })
  })

  it('reviews allow, deny, and inherit changes before saving and uses an empty global target', async () => {
    await render()
    const form = 'authorization-permission-form'
    for (const decision of ['allow', 'deny', 'inherit'] as const) {
      const callsBeforeReview = vi.mocked(guiAuthorizationManage).mock.calls.length
      choosePermission('board.create', decision)
      await act(async () => { submit(form); await settle() })
      expect(host.querySelector('[data-testid="authorization-change-summary"]')?.textContent).toContain(decision[0].toUpperCase() + decision.slice(1))
      expect(guiAuthorizationManage).toHaveBeenCalledTimes(callsBeforeReview)
      await act(async () => { button('authorization-confirm').click(); await settle() })
    }
    expect(guiAuthorizationManage).toHaveBeenNthCalledWith(1, 'permission.set', { subject_type: 'account', subject_id: member.id, operation: 'board.create', scope: 'global', target_id: '', decision: 'allow' })
    expect(guiAuthorizationManage).toHaveBeenNthCalledWith(2, 'permission.set', { subject_type: 'account', subject_id: member.id, operation: 'board.create', scope: 'global', target_id: '', decision: 'deny' })
    expect(guiAuthorizationManage).toHaveBeenNthCalledWith(3, 'permission.set', { subject_type: 'account', subject_id: member.id, operation: 'board.create', scope: 'global', target_id: '', decision: 'inherit' })
    expect(onAuthorizationChanged).toHaveBeenCalledTimes(3)
  })

  it('refuses permission changes for an empty non-global target and does not autosave', async () => {
    await render()
    await act(async () => { button('authorization-scope-thread').click(); await settle() })
    choosePermission('post.create', 'allow')
    expect(button('authorization-scope-thread').getAttribute('aria-pressed')).toBe('true')
    await act(async () => { submit('authorization-permission-form'); await settle() })
    expect(host.querySelector('[data-testid="authorization-mutation-error"]')?.textContent).toContain('Enter the target ID')
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
  })

  it('clears a reviewed permission change when a fresh capability read revokes the grant', async () => {
    await render()
    choosePermission('board.create', 'deny')
    await act(async () => { submit('authorization-permission-form'); await settle() })
    expect(host.querySelector('[data-testid="authorization-confirmation"]')).not.toBeNull()
    expect(guiAuthorizationManage).not.toHaveBeenCalled()

    authorization = { ...fullAuthorization, permission_set: false }
    await act(async () => { button('authorization-refresh').click(); await settle() })

    expect(host.querySelector('[data-testid="authorization-confirmation"]')).toBeNull()
    expect(host.querySelector('[data-testid="authorization-permission-form"]')).toBeNull()
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
  })

  it('blocks all writes when a creation receipt is pending or status lookup fails', async () => {
    creationStatus = pending
    await render()
    expect(host.querySelector('[data-testid="authorization-pending"]')?.textContent).toContain('creation request is pending')
    expect(button('authorization-role-create-toggle').disabled).toBe(true)
    expect(button('authorization-role-assignment-member-1-role-reviewer').disabled).toBe(true)
    expect(button('authorization-permission-review').disabled).toBe(true)
    expect(guiAuthorizationManage).not.toHaveBeenCalled()

    creationStatus = noPending
    vi.mocked(guiCreationStatus).mockRejectedValueOnce(new Error('receipt status unavailable'))
    await act(async () => { button('authorization-refresh').click(); await settle() })
    expect(host.querySelector('[data-testid="authorization-pending"]')).not.toBeNull()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('Could not load authorization data')
    expect(host.querySelector('[data-testid="authorization-role-create-toggle"]')).toBeNull()
    expect(guiAuthorizationManage).not.toHaveBeenCalled()
  })

  it('fails closed on authorization reads and exposes an explicit refresh retry', async () => {
    vi.mocked(guiRead).mockRejectedValueOnce(new Error('capability service unavailable'))
    await render()
    expect(host.querySelector('[data-testid="authorization-error"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="authorization-role-create-toggle"]')).toBeNull()
    await act(async () => { button('authorization-retry').click(); await settle() })
    expect(host.querySelector('[data-testid="authorization-error"]')).toBeNull()
    expect(host.querySelector('[data-testid="authorization-role-create-toggle"]')).not.toBeNull()
  })

  it('requires a fresh authorization read after an uncertain write instead of retrying it', async () => {
    vi.mocked(guiAuthorizationManage).mockRejectedValueOnce(new Error('write response lost'))
    await render()
    await act(async () => { button('authorization-role-create-toggle').click(); await settle() })
    setInput('authorization-role-name', 'Auditors')
    await act(async () => { submit('authorization-role-create-form'); await settle() })
    await act(async () => { button('authorization-confirm').click(); await settle() })
    expect(host.querySelector('[data-testid="authorization-mutation-error"]')?.textContent).toContain('result could not be confirmed')
    expect(guiAuthorizationManage).toHaveBeenCalledTimes(1)
    expect(button('authorization-role-create-submit').disabled).toBe(true)
    await act(async () => { button('authorization-refresh').click(); await settle() })
    expect(button('authorization-role-create-submit').disabled).toBe(false)
    expect(guiAuthorizationManage).toHaveBeenCalledTimes(1)
  })

  it('refreshes global authorization after an uncertain permission write may have committed', async () => {
    vi.mocked(guiAuthorizationManage).mockRejectedValueOnce(new Error('permission write response lost'))
    await render()
    choosePermission('board.create', 'deny')
    await act(async () => { submit('authorization-permission-form'); await settle() })
    await act(async () => { button('authorization-confirm').click(); await settle() })

    expect(guiAuthorizationManage).toHaveBeenCalledTimes(1)
    expect(guiAuthorizationManage).toHaveBeenCalledWith('permission.set', expect.objectContaining({ decision: 'deny' }))
    expect(onAuthorizationChanged).toHaveBeenCalledTimes(1)
    expect(host.querySelector('[data-testid="authorization-mutation-error"]')?.textContent).toContain('result could not be confirmed')
    expect(endOperation).toHaveBeenCalledTimes(1)
  })

  it('releases the shared App mutex if the authorization refresh callback fails or finishes after unmount', async () => {
    onAuthorizationChanged.mockRejectedValueOnce(new Error('workspace refresh failed'))
    await render()
    await act(async () => { button('authorization-role-create-toggle').click(); await settle() })
    setInput('authorization-role-name', 'Workspace auditors')
    await act(async () => { submit('authorization-role-create-form'); await settle() })
    await act(async () => { button('authorization-confirm').click(); await settle() })
    expect(onAuthorizationChanged).toHaveBeenCalledTimes(1)
    expect(endOperation).toHaveBeenCalledTimes(1)

    const refresh = deferred<void>()
    onAuthorizationChanged.mockReturnValueOnce(refresh.promise)
    endOperation.mockClear()
    await act(async () => { button('authorization-refresh').click(); await settle() })
    await act(async () => { submit('authorization-role-create-form'); await settle() })
    await act(async () => { button('authorization-confirm').click(); await settle() })
    expect(onAuthorizationChanged).toHaveBeenCalledTimes(2)
    await act(async () => root.unmount())
    await act(async () => { refresh.reject(new Error('unmounted callback failed')); await refresh.promise.catch(() => undefined); await settle() })
    expect(endOperation).toHaveBeenCalledTimes(1)
  })

  it('ignores a delayed role-list read after switching authorization accounts', async () => {
    let resolveOldList!: (result: { items: Account[] }) => void
    let accountListCall = 0
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'authorization.capabilities') return authorization as never
      if (operation === 'management.capabilities') return management as never
      if (operation === 'account.list') {
        accountListCall += 1
        if (accountListCall === 1) return new Promise((resolve) => { resolveOldList = resolve }) as never
        return { items: [{ ...member, id: 'account-b', name: 'Account B subject' }] } as never
      }
      if (operation === 'role.list') return { items: serverRoles } as never
      if (operation === 'permission.list') return { items: [] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    expect(host.querySelector('[data-testid="authorization-loading"]')).not.toBeNull()
    await rerender('account-b')
    expect(host.querySelector('[data-testid="authorization-account-row-account-b"]')?.textContent).toContain('Account B subject')
    await act(async () => { resolveOldList({ items: [{ ...member, id: 'account-a', name: 'Stale Account A subject' }] }); await settle() })
    expect(host.textContent).toContain('Account B subject')
    expect(host.textContent).not.toContain('Stale Account A subject')
  })

  it('ignores a delayed authorization read after unmount', async () => {
    const read = deferred<AuthorizationCapabilities>()
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'authorization.capabilities') return read.promise as never
      if (operation === 'management.capabilities') return management as never
      if (operation === 'account.list') return { items: serverAccounts } as never
      if (operation === 'role.list') return { items: serverRoles } as never
      if (operation === 'permission.list') return { items: [] } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await render()
    await act(async () => root.unmount())
    await act(async () => { read.resolve(fullAuthorization); await read.promise; await settle() })
    expect(host.querySelector('[data-testid="authorization-management"]')).toBeNull()
    expect(host.querySelector('[data-testid="authorization-role-list"]')).toBeNull()
  })
})
